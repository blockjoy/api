mod deserialize;
mod serialize;

use crate::grpc::blockjoy::commands_server::{Commands, CommandsServer};
use crate::grpc::blockjoy::{UpdateCommandResultRequest, UpdateCommandResultResponse};
use tonic::codegen::InterceptedService;
use tonic::{Request, Response, Status};
use crate::models::DbPool;

pub mod blockjoy {
    tonic::include_proto!("blockjoy.api");
}

pub struct CommandsServerImpl {
    db: DbPool,
}

impl CommandsServerImpl {
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl Commands for CommandsServerImpl {
    async fn update_command(
        &self,
        _request: Request<UpdateCommandResultRequest>,
    ) -> Result<Response<UpdateCommandResultResponse>, Status> {
        unimplemented!()
    }
}

pub fn server(
    db: DbPool,
) -> InterceptedService<
    CommandsServer<CommandsServerImpl>,
    fn(Request<()>) -> Result<Request<()>, Status>,
> {
    let service = CommandsServerImpl::new(db);

    CommandsServer::with_interceptor(service, authenticate_bearer)
}

/// Authenticate user identified by Bearer token
///
/// # Params
/// ***req:*** The incoming request. If successful, the user will be added to the request as
///     extension
#[allow(dead_code)]
fn authenticate_bearer(mut _req: Request<()>) -> Result<Request<()>, Status> {
    Ok(_req)
}
