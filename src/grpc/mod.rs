use tonic::{Request, Status};

pub mod blockjoy {
    tonic::include_proto!("blockjoy.api");
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