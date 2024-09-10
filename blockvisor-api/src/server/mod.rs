use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::header::CONTENT_TYPE;
use axum::http::HeaderValue;
use displaydoc::Display;
use thiserror::Error;
use tokio::net::TcpListener;
use tower::make::Shared;
use tower::steer::Steer;

use crate::config::Context;
use crate::{grpc, http};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to bind to `{0}`: `{1}`
    Listener(SocketAddr, tokio::io::Error),
    /// Server error: {0}
    Server(std::io::Error),
}

pub async fn start(context: Arc<Context>) -> Result<(), Error> {
    let addr = context.config.server.addr();
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|err| Error::Listener(addr, err))?;

    start_with_listener(context, listener).await
}

pub async fn start_with_listener(
    context: Arc<Context>,
    listener: TcpListener,
) -> Result<(), Error> {
    #[allow(deprecated)] // routes is a private field
    let grpc = grpc::server(&context).into_router();
    let http = http::router(&context);

    let service = Steer::new(vec![grpc, http], |req: &Request, _services: &[_]| {
        #[allow(clippy::bool_to_int_with_if)]
        if is_grpc_request(req) {
            0
        } else {
            1
        }
    });

    axum::serve(listener, Shared::new(service))
        .await
        .map_err(Error::Server)
}

fn is_grpc_request<B>(req: &Request<B>) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .map(HeaderValue::as_bytes)
        .filter(|content_type| content_type.starts_with(b"application/grpc"))
        .is_some()
}
