use crate::auth::FindableById;
use crate::auth::UserAuthToken;
use crate::errors::Result;
use crate::grpc::blockjoy_ui::command_service_server::CommandService;
use crate::grpc::blockjoy_ui::{CommandRequest, CommandResponse, Parameter, ResponseMeta};
use crate::grpc::helpers::try_get_token;
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
    pub fn new(db: models::DbPool, notifier: Notifier) -> Self {
        Self { db, notifier }
    }

    async fn handle_request(
        &self,
        req: Request<CommandRequest>,
        cmd_type: HostCmd,
    ) -> Result<CommandResponse> {
        let token = try_get_token::<_, UserAuthToken>(&req)?.try_into()?;
        let inner = req.into_inner();

        let mut tx = self.db.begin().await?;
        let cmd = self
            .create_command(inner.id.parse()?, cmd_type, inner.params, &mut tx)
            .await?;

        let response = CommandResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token)).with_message(cmd.id)),
        };
        let cmd = convert::db_command_to_grpc_command(&cmd, &mut tx).await?;
        self.send_notification(cmd).await?;
        tx.commit().await?;

        Ok(response)
    }

    async fn create_command(
        &self,
        host_id: Uuid,
        cmd: HostCmd,
        params: Vec<Parameter>,
        tx: &mut models::DbTrx<'_>,
    ) -> Result<models::Command, Status> {
        let resource_id = Self::get_resource_id_from_params(params)?;
        let req = DbCommandRequest {
            cmd,
            sub_cmd: None,
            resource_id,
        };

        let db_cmd = Command::create(host_id, req, tx).await?;
        let grpc_cmd = convert::db_command_to_grpc_command(&db_cmd, tx).await?;
        self.notifier.bv_commands_sender()?.send(&grpc_cmd).await?;

        match cmd {
            HostCmd::RestartNode | HostCmd::KillNode => {
                let node = models::Node::find_by_id(resource_id, tx).await?;
                self.notifier
                    .bv_nodes_sender()?
                    .send(&node.clone().into())
                    .await?;
                self.notifier
                    .ui_nodes_sender()?
                    .send(&node.try_into()?)
                    .await?;
            }
            _ => {}
        }

        Ok(db_cmd)
    }

    async fn send_notification(&self, command: blockjoy::Command) -> Result<()> {
        tracing::debug!("Sending notification: {:?}", command);
        println!("Sending notification: {:?}", command);
        self.notifier.bv_commands_sender()?.send(&command).await?;
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

#[tonic::async_trait]
impl CommandService for CommandServiceImpl {
    async fn create_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        println!("Command 'Create node' called!");
        let cmd = self.handle_request(request, HostCmd::CreateNode).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::DeleteNode).await?;
        Ok(Response::new(cmd))
    }

    async fn start_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::ShutdownNode).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_node(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::RestartNode).await?;
        Ok(Response::new(cmd))
    }

    async fn create_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::CreateBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn delete_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::RemoveBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn start_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn stop_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::StopBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn restart_host(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let cmd = self.handle_request(request, HostCmd::RestartBVS).await?;
        Ok(Response::new(cmd))
    }

    async fn execute_generic(
        &self,
        _request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}
