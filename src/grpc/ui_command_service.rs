use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::command_service_server::CommandService;
use crate::grpc::blockjoy_ui::{CommandRequest, CommandResponse, Parameter, ResponseMeta};
use crate::grpc::notification;
use crate::models;
use crate::models::{Command, CommandRequest as DbCommandRequest, HostCmd};
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct CommandServiceImpl {
    db: models::DbPool,
}

impl CommandServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }

    async fn create_command(
        &self,
        host_id: Uuid,
        cmd: HostCmd,
        sub_cmd: Option<String>,
        params: Vec<Parameter>,
    ) -> Result<Command, Status> {
        let resource_id = Self::get_resource_id_from_params(params)?;
        let req = DbCommandRequest {
            cmd,
            sub_cmd,
            resource_id,
        };

        let mut tx = self.db.begin().await?;
        let cmd = Command::create(host_id, req, &mut tx).await?;
        tx.commit().await?;
        Ok(cmd)
    }

    async fn send_notification(&self, command: models::Command) -> Result<()> {
        tracing::debug!("Sending notification: {:?}", command);
        let notifier = notification::Notifier::new(self.db.clone());
        notifier
            .commands_sender(command.host_id)
            .send(command.id)
            .await
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

macro_rules! create_command {
    ($obj:expr, $req:expr, $cmd:expr, $sub_cmd:expr) => {{
        let inner = $req.into_inner();

        let host_id = inner.id;
        let cmd = $obj
            .create_command(
                Uuid::parse_str(host_id.as_str()).map_err(ApiError::from)?,
                $cmd,
                $sub_cmd,
                inner.params,
            )
            .await?;

        let response = CommandResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta).with_message(cmd.id)),
        };
        $obj.send_notification(cmd).await?;

        Ok(Response::new(response))
    }};
}

#[tonic::async_trait]
impl CommandService for CommandServiceImpl {
    async fn create_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::CreateNode, None }
    }

    async fn delete_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::DeleteNode, None }
    }

    async fn start_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::RestartNode, None }
    }

    async fn stop_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::ShutdownNode, None }
    }

    async fn restart_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::RestartNode, None }
    }

    async fn create_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::CreateBVS, None }
    }

    async fn delete_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::RemoveBVS, None }
    }

    async fn start_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::RestartBVS, None }
    }

    async fn stop_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::StopBVS, None }
    }

    async fn restart_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        create_command! { self, request, HostCmd::RestartBVS, None }
    }

    async fn execute_generic(
        &self,
        _request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
