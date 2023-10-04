use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::database::{Transaction, WriteConn};

use super::api::babel_service_server::BabelService;
use super::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Babel notify endpoint is not implemented.
    NotifyUnimplemented,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            NotifyUnimplemented => Status::unimplemented("Unimplemeneted."),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BabelService for Grpc {
    async fn notify(
        &self,
        req: Request<api::BabelServiceNotifyRequest>,
    ) -> Result<Response<api::BabelServiceNotifyResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| notify(req, meta, write).scope_boxed())
            .await
    }
}

#[allow(clippy::unused_async)]
async fn notify(
    _req: api::BabelServiceNotifyRequest,
    _meta: MetadataMap,
    _write: WriteConn<'_, '_>,
) -> Result<api::BabelServiceNotifyResponse, Error> {
    Err(Error::NotifyUnimplemented)
}
