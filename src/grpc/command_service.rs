use crate::auth::FindableById;
use crate::errors::ApiError;
use crate::grpc::blockjoy::commands_server::Commands;
use crate::grpc::blockjoy::{Command, CommandInfo, CommandResponse, PendingCommandsRequest};
use crate::grpc::convert;
use crate::grpc::convert::db_command_to_grpc_command;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncPgConnection;
use std::str::FromStr;
use tonic::{Request, Response, Status};

impl CommandInfo {
    fn as_update(&self) -> crate::Result<models::UpdateCommand<'_>> {
        Ok(models::UpdateCommand {
            id: self.id.parse()?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: chrono::Utc::now(),
        })
    }
}

#[tonic::async_trait]
impl Commands for super::GrpcImpl {
    async fn get(&self, request: Request<CommandInfo>) -> Result<Response<Command>, Status> {
        let inner = request.into_inner();
        let cmd_id = uuid::Uuid::from_str(inner.id.as_str()).map_err(ApiError::from)?;
        let mut db_conn = self.conn().await?;
        let cmd = models::Command::find_by_id(cmd_id, &mut db_conn).await?;
        let grpc_cmd = db_command_to_grpc_command(&cmd, &mut db_conn).await?;
        let response = Response::new(grpc_cmd);

        Ok(response)
    }

    /// This endpoint receives all ack responses from blockvisord, and updates the status of the
    /// emitted commands with the provided outcome.
    async fn update(&self, request: Request<CommandInfo>) -> Result<Response<()>, Status> {
        let inner = request.into_inner();
        let update_cmd = dbg!(inner.as_update())?;
        self.trx(|c| {
            async move {
                let cmd = dbg!(update_cmd.update(c).await)?;
                match dbg!(cmd.exit_status) {
                    Some(0) => {
                        // Some responses require us to register success.
                        register_success(cmd, c).await;
                    }
                    // Will match any integer other than 0.
                    Some(_) => {
                        // We got back an error status code. In practice, blockvisord sends 0 for
                        // success and 1 for failure, but we treat every non-zero exit code as an
                        // error, not just 1.
                        recover(self, cmd, c).await;
                    }
                    None => {}
                }
                Ok(())
            }
            .scope_boxed()
        })
        .await?;

        Ok(Response::new(()))
    }

    async fn pending(
        &self,
        request: Request<PendingCommandsRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let inner = request.into_inner();
        let host_id = inner.host_id.parse().map_err(ApiError::from)?;
        let mut db_conn = self.conn().await?;
        let cmds = models::Command::find_pending_by_host(host_id, &mut db_conn).await?;
        let mut response = CommandResponse { commands: vec![] };

        for cmd in cmds {
            let grpc_cmd = db_command_to_grpc_command(&cmd, &mut db_conn).await?;
            response.commands.push(grpc_cmd);
        }

        Ok(Response::new(response))
    }
}

/// Some endpoints require some additional action from us when we recieve a success message back
/// from blockvisord. For now this is limited to creating a node_deployment_logs entry when
/// CreateNode has succeeded, but this may expand over time.
async fn register_success(succeeded: models::Command, conn: &mut AsyncPgConnection) {
    match succeeded.cmd {
        models::CommandType::CreateNode => create_node_success(succeeded, conn).await,
        _ => {}
    }
}

/// In case of a successful node deployment, we are expected to write node_deployment_logs entry to
/// the database. The `action` we pass in is `SuccessReceived`.
async fn create_node_success(succeeded: models::Command, conn: &mut AsyncPgConnection) {
    let Some(node_id) = succeeded.node_id else {
        tracing::error!("`CreateNode` command has no node id!");
        return;
    };
    let Ok(node) = models::Node::find_by_id(node_id, conn).await else {
        tracing::error!("Could not get node for node_id {node_id}");
        return;
    };
    let new_log = models::NewNodeDeploymentLog {
        host_id: node.host_id,
        node_id,
        action: models::NodeDeploymentAction::SuccessReceived,
        blockchain_id: node.blockchain_id,
        node_type: node.node_type,
        version: node.version.as_deref().unwrap_or("latest"),
        created_at: chrono::Utc::now(),
    };
    let _ = new_log.create(conn).await;
}

/// When we get a failed command back from blockvisord, we can try to recover from this. This is
/// currently only implemented for failed node creates. Note that this function largely ignores
/// errors. We are already in a state where we are trying to recover from a failure mode, so we will
/// make our best effort to recover. If a command won't send but it not essential for process, we
/// ignore and continue.
async fn recover(impler: &super::GrpcImpl, failed: models::Command, conn: &mut AsyncPgConnection) {
    match failed.cmd {
        models::CommandType::CreateNode => recover_created(impler, failed, conn).await,
        _ => {}
    }
}

async fn recover_created(
    impler: &super::GrpcImpl,
    failed_cmd: models::Command,
    conn: &mut AsyncPgConnection,
) {
    let Some(node_id) = dbg!(failed_cmd.node_id) else {
        tracing::error!("`CreateNode` command has no node id!");
        return;
    };
    // Recovery from a failed delete looks like this:
    // 1. Send a message to blockvisord to delete the old node.
    // 2. Log that our creation has failed.
    // 3. Decide whether and where to re-create the node:
    //    a. If this is the first failure on the current host, we try again with the same host.
    //    b. Otherwise, if this is the first host we tried on, we try again with a new host.
    //    c. Otherwise, we cannot recover.
    // 4. Use the previous decision to send a new create message to the right instance of
    //    blockvisord, or mark the current node as failed and send an MQTT message to the front end.
    let Ok(mut node) = dbg!(models::Node::find_by_id(node_id, conn).await) else {
        tracing::error!("Could not get node for node_id {node_id}");
        return;
    };

    // 1. We send a delete to blockvisord to help it with cleanup.
    send_delete(impler, &node, conn).await;

    // 2. We make a note in the node_deployment_logs table that creating our node failed. This may
    //    be unexpected, but we abort here when we fail to create that log. This is because the logs
    //    table is used to decide whether or not to retry. If logging our result failed, we may end
    //    up in an infinite loop.
    let new_log = models::NewNodeDeploymentLog {
        host_id: node.host_id,
        node_id,
        action: models::NodeDeploymentAction::FailureReceived,
        blockchain_id: node.blockchain_id,
        node_type: node.node_type,
        version: node.version.as_deref().unwrap_or("latest"),
        created_at: chrono::Utc::now(),
    };
    let Ok(_) = dbg!(new_log.create(conn).await) else {
        tracing::error!("Failed to create deployment log entry!");
        return;
    };

    // 3. We now find the host that is next in line, and assign our node to that host.
    let Ok(host) = node.find_host(conn).await else {
        tracing::warn!("Could not reschedule node on a new host, system is out of resources");
        return;
    };
    node.host_id = host.id;
    let Ok(node) = dbg!(node.update(conn).await) else {
        tracing::error!("Could not update node!");
        return;
    };

    // 4. We notify blockvisor of our retry via an MQTT message.
    let _ = super::ui_node_service::create_notification(impler, &node, conn).await;
    // we also start the node.
    let _ = super::ui_node_service::start_notification(impler, &node, conn).await;
}

/// Send a delete message to blockvisord, to delete the given node. We do this to assist blockvisord
/// to clean up after a failed node create.
async fn send_delete(impler: &super::GrpcImpl, node: &models::Node, conn: &mut AsyncPgConnection) {
    let cmd = models::NewCommand {
        host_id: node.host_id,
        cmd: models::CommandType::DeleteNode,
        sub_cmd: None,
        node_id: Some(node.id),
    };
    let Ok(cmd) = cmd.create(conn).await else {
        tracing::error!("Could not create node delete command while recovering");
        return;
    };
    let Ok(cmd) = convert::db_command_to_grpc_command(&cmd, conn).await else {
        tracing::error!("Could not convert node delete command to gRPC repr while recovering");
        return;
    };
    let Ok(mut sender) = impler.notifier.bv_commands_sender() else { return; };
    let _ = sender.send(&cmd).await;
}
