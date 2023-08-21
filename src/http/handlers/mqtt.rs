use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, Router};
use axum_extra::extract::WithRejection;
use displaydoc::Display;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, error};

use crate::config::Context;
use crate::database::Database;
use crate::http::response;
use crate::mqtt::handler::{self, AclRequest};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Database error: {0}
    Database(#[from] crate::database::Error),
    /// MQTT handler error: {0}
    Handler(#[from] handler::Error),
    /// Failed to parse JSON: {0}
    ParseJson(#[from] JsonRejection),
    /// Failed to parse RequestToken: {0}
    ParseRequestToken(crate::auth::token::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        error!("{}: {self}", std::any::type_name::<Error>());

        use Error::*;
        match self {
            Auth(_) | Handler(handler::Error::Claims(_)) | ParseRequestToken(_) => {
                response::unauthorized().into_response()
            }
            Database(_) => response::failed().into_response(),
            Handler(_) => response::bad_params().into_response(),
            ParseJson(rejection) => (rejection.status(), rejection.body_text()).into_response(),
        }
    }
}

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/acl", post(acl))
        .route("/auth", post(auth))
        .with_state(context)
}

async fn auth(WithRejection(value, _): WithRejection<Json<Value>, Error>) -> impl IntoResponse {
    debug!("MQTT auth payload: {value:?}");
    response::ok()
}

async fn acl(
    State(ctx): State<Arc<Context>>,
    WithRejection(req, _): WithRejection<Json<AclRequest>, Error>,
) -> Result<impl IntoResponse, Error> {
    let mut conn = ctx.pool.conn().await?;

    let token = req.username.parse().map_err(Error::ParseRequestToken)?;
    let claims = ctx.auth.claims_from_token(&token, &mut conn).await?;

    req.allow(claims, &mut conn).await?;

    Ok(response::ok())
}
