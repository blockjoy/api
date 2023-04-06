use super::{blockjoy, blockjoy_ui};
use crate::auth::FindableById;
use crate::auth::UserAuthToken;
use crate::grpc::blockjoy_ui::command_service_server::CommandService;
use crate::grpc::helpers::try_get_token;
use crate::grpc::notification::Notifier;
use crate::models;
use crate::models::CommandType::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};
use uuid::Uuid;

async fn handle_request(
    impler: &super::GrpcImpl,
    req: Request<blockjoy_ui::CommandRequest>,
    cmd_type: models::CommandType,
) -> crate::Result<blockjoy_ui::CommandResponse> {
    let token = try_get_token::<_, UserAuthToken>(&req)?.try_into()?;
    let inner = req.into_inner();

    let notifier = impler.notifier.clone();
    let host_id = inner.id.parse()?;
    impler
        .trx(|c| {
            async move {
                let cmd = create_command(host_id, cmd_type, inner.params, notifier, c).await?;
                let response = blockjoy_ui::CommandResponse {
                    meta: Some(
                        blockjoy_ui::ResponseMeta::from_meta(inner.meta, Some(token))
                            .with_message(cmd.id),
                    ),
                };
                let cmd = blockjoy::Command::from_model(&cmd, c).await?;
                send_notification(cmd, &impler.notifier).await?;

                Ok(response)
            }
            .scope_boxed()
        })
        .await
}

async fn create_command(
    host_id: Uuid,
    cmd: models::CommandType,
    params: Vec<blockjoy_ui::Parameter>,
    notifier: Notifier,
    conn: &mut diesel_async::AsyncPgConnection,
) -> crate::Result<models::Command> {
    let new_command = models::NewCommand {
        host_id,
        cmd,
        sub_cmd: None,
        node_id: cmd
            .is_node_specific()
            .then(|| get_resource_id_from_params(params))
            .transpose()?,
    };

    let db_cmd = new_command.create(conn).await?;
    match cmd {
        models::CommandType::RestartNode | models::CommandType::KillNode => {
            // RestartNode and KillNode are node-specific, so unwrap below is safe:
            let node_id = db_cmd
                .node_id
                .expect("RestartNode and KillNode must be node-specific!");

            let grpc_cmd = blockjoy::Command::from_model(&db_cmd, conn).await?;
            notifier.bv_commands_sender()?.send(&grpc_cmd).await?;
            let node = models::Node::find_by_id(node_id, conn).await?;
            notifier
                .bv_nodes_sender()?
                .send(&blockjoy::Node::from_model(node.clone()))
                .await?;
        }
        _ => {}
    }
    Ok(db_cmd)
}

async fn send_notification(command: blockjoy::Command, notifier: &Notifier) -> crate::Result<()> {
    tracing::debug!("Sending notification: {:?}", command);
    notifier.bv_commands_sender()?.send(&command).await?;
    Ok(())
}

fn get_resource_id_from_params(params: Vec<blockjoy_ui::Parameter>) -> Result<Uuid, Status> {
    let bad_uuid = |_| Status::invalid_argument("Malformatted uuid");
    params
        .into_iter()
        .find(|p| p.name == "resource_id")
        .ok_or_else(|| Status::internal("Resource ID not available"))
        .and_then(|val| val.value.parse().map_err(bad_uuid))
}

#[tonic::async_trait]
impl CommandService for super::GrpcImpl {
    async fn create_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, CreateNode).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, DeleteNode).await?;
        Ok(Response::new(cmd))
    }

    async fn start_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, ShutdownNode).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn create_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, CreateBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, RemoveBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn start_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, StopBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = handle_request(self, request, RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn execute_generic(
        &self,
        _request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
