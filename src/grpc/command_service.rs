use crate::auth::FindableById;
use crate::errors::ApiError;
use crate::grpc::blockjoy::commands_server::Commands;
use crate::grpc::blockjoy::{Command, CommandInfo, CommandResponse, PendingCommandsRequest};
use crate::grpc::convert::db_command_to_grpc_command;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use std::str::FromStr;
use tonic::{Request, Response, Status};

pub struct CommandsServiceImpl {
    db: models::DbPool,
}

impl CommandsServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

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
impl Commands for CommandsServiceImpl {
    async fn get(&self, request: Request<CommandInfo>) -> Result<Response<Command>, Status> {
        let inner = request.into_inner();
        let cmd_id = uuid::Uuid::from_str(inner.id.as_str()).map_err(ApiError::from)?;
        let mut db_conn = self.db.conn().await?;
        let cmd = models::Command::find_by_id(cmd_id, &mut db_conn).await?;
        let grpc_cmd = db_command_to_grpc_command(&cmd, &mut db_conn).await?;
        let response = Response::new(grpc_cmd);

        Ok(response)
    }

    async fn update(&self, request: Request<CommandInfo>) -> Result<Response<()>, Status> {
        let inner = request.into_inner();
        self.db
            .trx(|c| {
                async move {
                    let update_cmd = inner.as_update()?;
                    update_cmd.update(c).await?;
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
        let host_id = uuid::Uuid::parse_str(inner.host_id.as_str()).map_err(ApiError::from)?;
        let mut db_conn = self.db.conn().await?;
        let cmds = models::Command::find_pending_by_host(host_id, &mut db_conn).await?;
        let mut response = CommandResponse { commands: vec![] };

        for cmd in cmds {
            let grpc_cmd = db_command_to_grpc_command(&cmd, &mut db_conn).await?;
            response.commands.push(grpc_cmd);
        }

        Ok(Response::new(response))
    }
}
