use crate::auth::FindableById;
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::command_service_server::CommandService;
use crate::grpc::blockjoy_ui::{CommandRequest, CommandResponse, Parameter, ResponseMeta};
use crate::grpc::notification::Notifier;
use crate::models;
use crate::models::{Command, CommandRequest as DbCommandRequest, HostCmd};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::{blockjoy, convert};

pub struct CommandServiceImpl {
    db: models::DbPool,
    notifier: Notifier,
}

impl CommandServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self {
            db,
            notifier: Notifier::new(),
        }
    }

    async fn create_command(
        &self,
        host_id: Uuid,
        cmd: HostCmd,
        sub_cmd: Option<String>,
        params: Vec<Parameter>,
        tx: &mut models::DbTrx<'_>,
    ) -> Result<models::Command, Status> {
        let resource_id = Self::get_resource_id_from_params(params)?;
        let req = DbCommandRequest {
            cmd,
            sub_cmd,
            resource_id,
        };

        let db_cmd = Command::create(host_id, req, tx).await?;

        match cmd {
            HostCmd::RestartNode | HostCmd::KillNode => {
                let node = models::Node::find_by_id(resource_id, tx).await?;
                self.notifier
                    .bv_nodes_sender()
                    .send(&node.clone().into())
                    .await;
                self.notifier
                    .ui_nodes_sender()
                    .send(&node.try_into()?)
                    .await;
                // self.notifier.ui_commands_sender().send(&cmd).await;
            }
            _ => {}
        }

        Ok(db_cmd)
    }

    async fn send_notification(&self, command: blockjoy::Command) -> Result<()> {
        tracing::debug!("Sending notification: {:?}", command);
        self.notifier.bv_commands_sender().send(&command).await;
        Ok(())
    }

    fn get_resource_id_from_params(params: Vec<Parameter>) -> Result<Uuid, Status> {
        let bad_uuid = |_| Status::invalid_argument("Malformatted uuid");
        params
            .into_iter()
            .find(|p| p.name == "resource_id")
            .ok_or_else(|| Status::internal("Resource ID not available"))
            .and_then(|val| Uuid::parse_str(val.value.as_str()).map_err(bad_uuid))
    }
}

async fn create_command(
    impler: &CommandServiceImpl,
    req: Request<CommandRequest>,
    cmd_type: HostCmd,
) -> Result<Response<CommandResponse>, Status> {
    let inner = req.into_inner();

    let host_id = inner.id;
    let mut tx = impler.db.begin().await?;
    let cmd = impler
        .create_command(
            Uuid::parse_str(host_id.as_str()).map_err(ApiError::from)?,
            cmd_type,
            None,
            inner.params,
            &mut tx,
        )
        .await?;

    let response = CommandResponse {
        meta: Some(ResponseMeta::from_meta(inner.meta).with_message(cmd.id)),
    };
    let cmd = convert::db_command_to_grpc_command(&cmd, &mut tx).await?;
    impler.send_notification(cmd).await?;
    tx.commit().await?;

    Ok(Response::new(response))
}

#[tonic::async_trait]
impl CommandService for CommandServiceImpl {
    async fn create_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::CreateNode).await
    }

    async fn delete_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::DeleteNode).await
    }

    async fn start_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::RestartNode).await
    }

    async fn stop_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::ShutdownNode).await
    }

    async fn restart_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::RestartNode).await
    }

    async fn create_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::CreateBVS).await
    }

    async fn delete_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::RemoveBVS).await
    }

    async fn start_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::RestartBVS).await
    }

    async fn stop_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::StopBVS).await
    }

    async fn restart_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command(self, request, HostCmd::RestartBVS).await
    }

    async fn execute_generic(
        &self,
        _request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
