mod hybrid;

use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;

use crate::config::Context;
use crate::{grpc, http};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Server error: {0}
    Server(hyper::Error),
}

pub async fn start(context: Arc<Context>) -> Result<(), Error> {
    let http = http::router(&context).into_make_service();
    let grpc = grpc::server(&context).into_service();
    let both = hybrid::hybrid(http, grpc);

    axum::Server::bind(&context.config.database.bind_addr())
        .serve(both)
        .await
        .map_err(Error::Server)
}
