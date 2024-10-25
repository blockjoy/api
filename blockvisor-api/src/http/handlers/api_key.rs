#![allow(unused)]

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{post, Router};
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tracing::{debug, error};

use crate::auth::resource::{OrgId, UserId};
use crate::config::Context;
use crate::database::{Transaction, WriteConn};
use crate::grpc::{self, api};
use crate::model::{self, User};
use crate::stripe::api::event;

impl From<grpc::api_key::Error> for super::Error {
    fn from(err: grpc::api_key::Error) -> super::Error {
        use grpc::api_key::Error::*;
        let (message, status) = match err {
            ClaimsNotUser => ("Access denied.", 403),
            Diesel(_) | MissingUpdatedAt => ("Internal error.", 500),
            MissingCreateScope => ("scope", 400),
            MissingScopeResourceId | ParseResourceId(_) => ("resource_id", 400),
            NothingToUpdate => ("Nothing to update.", 400),
            ParseKeyId(_) => ("id", 400),
            ParseResourceType(_) => ("resource", 400),
            Auth(err) => return err.into(),
            Claims(err) => return err.into(),
            Model(err) => return todo!(),
        };
        error!("{err}");

        super::Error::new(message, status)
    }
}

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new().route("/", post(create)).with_state(context)
}

async fn create(
    State(ctx): State<Arc<Context>>,
    headers: axum::http::header::HeaderMap,
    axum::Json(req): axum::Json<api::ApiKeyServiceCreateRequest>,
) -> Result<axum::Json<api::ApiKeyServiceCreateResponse>, super::Error> {
    ctx.write(|write| grpc::api_key::create(req, headers.into(), write).scope_boxed())
        .await
}
