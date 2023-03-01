use super::{blockjoy, blockjoy_ui, convert};
use crate::auth::FindableById;
use crate::auth::UserAuthToken;
use crate::errors::Result;
use crate::grpc::blockjoy_ui::command_service_server::CommandService;
use crate::grpc::helpers::try_get_token;
use crate::grpc::notification::Notifier;
use crate::models;
use crate::models::HostCmd::*;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct CommandServiceImpl {
    db: models::DbPool,
    notifier: Notifier,
}

impl CommandServiceImpl {
    pub fn new(db: models::DbPool, notifier: Notifier) -> Self {
        Self { db, notifier }
    }

    async fn handle_request(
        &self,
        req: Request<blockjoy_ui::CommandRequest>,
        cmd_type: models::HostCmd,
    ) -> Result<blockjoy_ui::CommandResponse> {
        let token = try_get_token::<_, UserAuthToken>(&req)?.try_into()?;
        let inner = req.into_inner();

        let notifier = self.notifier.clone();
        let host_id = inner.id.parse()?;
        self.db
            .trx(|c| {
                async move {
                    let cmd = create_command(host_id, cmd_type, inner.params, notifier, c).await?;
                    let response = blockjoy_ui::CommandResponse {
                        meta: Some(
                            blockjoy_ui::ResponseMeta::from_meta(inner.meta, Some(token))
                                .with_message(cmd.id),
                        ),
                    };
                    let cmd = convert::db_command_to_grpc_command(&cmd, c).await?;
                    send_notification(cmd, &self.notifier).await?;

                    Ok(response)
                }
                .scope_boxed()
            })
            .await
    }
}

async fn create_command(
    host_id: Uuid,
    cmd: models::HostCmd,
    params: Vec<blockjoy_ui::Parameter>,
    notifier: Notifier,
    conn: &mut diesel_async::AsyncPgConnection,
) -> Result<models::Command> {
    let resource_id = get_resource_id_from_params(params)?;
    let new_command = models::NewCommand {
        host_id,
        cmd,
        sub_cmd: None,
        resource_id,
    };

    let db_cmd = new_command.create(conn).await?;
    match cmd {
        models::HostCmd::RestartNode | models::HostCmd::KillNode => {
            let grpc_cmd = convert::db_command_to_grpc_command(&db_cmd, conn).await?;
            notifier.bv_commands_sender()?.send(&grpc_cmd).await?;
            let node = models::Node::find_by_id(resource_id, conn).await?;
            notifier
                .bv_nodes_sender()?
                .send(&blockjoy::NodeInfo::from_model(node.clone()))
                .await?;
            notifier.ui_nodes_sender()?.send(&node.try_into()?).await?;
        }
        _ => {}
    }
    Ok(db_cmd)
}

async fn send_notification(command: blockjoy::Command, notifier: &Notifier) -> Result<()> {
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
        .and_then(|val| Uuid::parse_str(val.value.as_str()).map_err(bad_uuid))
}

#[tonic::async_trait]
impl CommandService for CommandServiceImpl {
    async fn create_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, CreateNode).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, DeleteNode).await?;
        Ok(Response::new(cmd))
    }

    async fn start_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, ShutdownNode).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_node(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn create_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, CreateBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, RemoveBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn start_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, StopBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_host(
        &self,
        request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        let cmd = self.handle_request(request, RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn execute_generic(
        &self,
        _request: Request<blockjoy_ui::CommandRequest>,
    ) -> Result<Response<blockjoy_ui::CommandResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
