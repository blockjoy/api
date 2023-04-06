use super::blockjoy::{self, commands_server::Commands};
use super::convert;
use super::helpers::required;
use crate::auth::FindableById;
use crate::models;
use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncPgConnection;
use std::str::FromStr;
use tonic::{Request, Response, Status};

impl blockjoy::CommandInfo {
    fn as_update(&self) -> crate::Result<models::UpdateCommand<'_>> {
        Ok(models::UpdateCommand {
            id: self.id.parse()?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: chrono::Utc::now(),
        })
    }
}

impl blockjoy::Parameter {
    fn new(name: &str, val: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: val.to_owned(),
        }
    }
}

impl blockjoy::Command {
    pub async fn from_model(
        model: &models::Command,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<blockjoy::Command> {
        use blockjoy::command::Type;
        use blockjoy::node_command::Command;
        use models::CommandType::*;

        // Extract the node id from the model, if there is one.
        let node_id = || model.node_id.ok_or_else(required("command.node_id"));
        // Closure to conveniently construct a blockjoy:: from the data that we need to have.
        let node_cmd = |command, node_id| {
            Ok(blockjoy::Command {
                r#type: Some(Type::Node(blockjoy::NodeCommand {
                    node_id,
                    host_id: model.host_id.to_string(),
                    command: Some(command),
                    api_command_id: model.id.to_string(),
                    created_at: Some(convert::try_dt_to_ts(model.created_at)?),
                })),
            })
        };
        // Construct a blockjoy::Command with the node id extracted from the `node.node_id` field.
        // Only `DeleteNode` does not use this method.
        let node_cmd_default_id = |command| node_cmd(command, node_id()?.to_string());

        match model.cmd {
            RestartNode => node_cmd_default_id(Command::Restart(blockjoy::NodeRestart {})),
            KillNode => node_cmd_default_id(Command::Stop(blockjoy::NodeStop {})),
            ShutdownNode => node_cmd_default_id(Command::Stop(blockjoy::NodeStop {})),
            UpdateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let cmd = Command::Update(blockjoy::NodeUpdate {
                    self_update: Some(node.self_update),
                });
                node_cmd_default_id(cmd)
            }
            MigrateNode => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            GetNodeVersion => node_cmd_default_id(Command::InfoGet(blockjoy::NodeGet {})),

            // The following should be HostCommands
            CreateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let image = blockjoy::ContainerImage {
                    protocol: blockchain.name,
                    node_type: node.node_type.to_string().to_lowercase(),
                    node_version: node.version.as_deref().unwrap_or("latest").to_lowercase(),
                    status: blockjoy::container_image::StatusName::Development.into(),
                };
                let network = blockjoy::Parameter::new("network", &node.network);
                let r#type = models::NodePropertiesWithId {
                    id: node.node_type.into(),
                    props: node.properties()?,
                };
                let properties = node
                    .properties()?
                    .iter_props()
                    .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                    .map(|(name, value)| blockjoy::Parameter::new(name, value))
                    .chain([network])
                    .collect();
                let cmd = Command::Create(blockjoy::NodeCreate {
                    name: node.name,
                    blockchain: node.blockchain_id.to_string(),
                    image: Some(image),
                    r#type: serde_json::to_string(&r#type)?,
                    ip: node.ip_addr,
                    gateway: node.ip_gateway,
                    self_update: node.self_update,
                    properties,
                });

                node_cmd_default_id(cmd)
            }
            DeleteNode => {
                let node_id = model
                    .sub_cmd
                    .clone()
                    .ok_or_else(required("command.node_id"))?;
                let cmd = Command::Delete(blockjoy::NodeDelete {});
                node_cmd(cmd, node_id)
            }
            GetBVSVersion => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            UpdateBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            RestartBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            RemoveBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            CreateBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            StopBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
        }
    }
}

#[tonic::async_trait]
impl Commands for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<blockjoy::CommandInfo>,
    ) -> Result<Response<blockjoy::Command>, Status> {
        let inner = request.into_inner();
        let cmd_id = uuid::Uuid::from_str(inner.id.as_str()).map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmd = models::Command::find_by_id(cmd_id, &mut db_conn).await?;
        let grpc_cmd = blockjoy::Command::from_model(&cmd, &mut db_conn).await?;
        let response = Response::new(grpc_cmd);

        Ok(response)
    }

    async fn update(
        &self,
        request: Request<blockjoy::CommandInfo>,
    ) -> Result<Response<()>, Status> {
        let inner = request.into_inner();
        let update_cmd = inner.as_update()?;
        self.trx(|c| {
            async move {
                let cmd = update_cmd.update(c).await?;
                match cmd.exit_status {
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
        request: Request<blockjoy::PendingCommandsRequest>,
    ) -> Result<Response<blockjoy::CommandResponse>, Status> {
        let inner = request.into_inner();
        let host_id = inner.host_id.parse().map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmds = models::Command::find_pending_by_host(host_id, &mut db_conn).await?;
        let mut response = blockjoy::CommandResponse { commands: vec![] };

        for cmd in cmds {
            let grpc_cmd = blockjoy::Command::from_model(&cmd, &mut db_conn).await?;
            response.commands.push(grpc_cmd);
        }

        Ok(Response::new(response))
    }
}

/// Some endpoints require some additional action from us when we recieve a success message back
/// from blockvisord. For now this is limited to creating a node_logs entry when
/// CreateNode has succeeded, but this may expand over time.
async fn register_success(succeeded: models::Command, conn: &mut AsyncPgConnection) {
    if succeeded.cmd == models::CommandType::CreateNode {
        create_node_success(succeeded, conn).await;
    }
}

/// In case of a successful node deployment, we are expected to write node_logs entry to
/// the database. The `event` we pass in is `Succeeded`.
async fn create_node_success(succeeded: models::Command, conn: &mut AsyncPgConnection) {
    let Some(node_id) = succeeded.node_id else {
        tracing::error!("`CreateNode` command has no node id!");
        return;
    };
    let Ok(node) = models::Node::find_by_id(node_id, conn).await else {
        tracing::error!("Could not get node for node_id {node_id}");
        return;
    };
    let Ok(blockchain) = models::Blockchain::find_by_id(node.blockchain_id, conn).await else {
        tracing::error!("Could not get blockchain for node {node_id}");
        return;
    };

    let new_log = models::NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: models::NodeLogEvent::Succeeded,
        blockchain_name: &blockchain.name,
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
    if failed.cmd == models::CommandType::CreateNode {
        recover_created(impler, failed, conn).await;
    }
}

async fn recover_created(
    impler: &super::GrpcImpl,
    failed_cmd: models::Command,
    conn: &mut AsyncPgConnection,
) {
    let Some(node_id) = failed_cmd.node_id else {
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
    let Ok(mut node) = models::Node::find_by_id(node_id, conn).await else {
        tracing::error!("Could not get node for node_id {node_id}");
        return;
    };
    let Ok(blockchain) = models::Blockchain::find_by_id(node.blockchain_id, conn).await else {
        tracing::error!("Could not get blockchain for node {node_id}");
        return;
    };

    // 1. We send a delete to blockvisord to help it with cleanup.
    send_delete(impler, &node, conn).await;

    // 2. We make a note in the node_logs table that creating our node failed. This may
    //    be unexpected, but we abort here when we fail to create that log. This is because the logs
    //    table is used to decide whether or not to retry. If logging our result failed, we may end
    //    up in an infinite loop.
    let new_log = models::NewNodeLog {
        host_id: node.host_id,
        node_id,
        event: models::NodeLogEvent::Failed,
        blockchain_name: &blockchain.name,
        node_type: node.node_type,
        version: node.version.as_deref().unwrap_or("latest"),
        created_at: chrono::Utc::now(),
    };
    let Ok(_) = new_log.create(conn).await else {
        tracing::error!("Failed to create deployment log entry!");
        return;
    };

    // 3. We now find the host that is next in line, and assign our node to that host.
    let Ok(host) = node.find_host(conn).await else {
        // We were unable to find a new host. This may happen because the system is out of resources
        // or because we have retried to many times. Either way we have to log that this retry was
        // canceled.
        let new_log = models::NewNodeLog {
            host_id: node.host_id,
            node_id,
            event: models::NodeLogEvent::Canceled,
            blockchain_name: &blockchain.name,
            node_type: node.node_type,
            version: node.version.as_deref().unwrap_or("latest"),
            created_at: chrono::Utc::now(),
        };
        let Ok(_) = new_log.create(conn).await else {
            tracing::error!("Failed to create cancelation log entry!");
            return;
        };
        return;
    };
    node.host_id = host.id;
    let Ok(node) = node.update(conn).await else {
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
    let Ok(cmd) = blockjoy::Command::from_model(&cmd, conn).await else {
        tracing::error!("Could not convert node delete command to gRPC repr while recovering");
        return;
    };
    let Ok(mut sender) = impler.notifier.bv_commands_sender() else { return; };
    let _ = sender.send(&cmd).await;
}
