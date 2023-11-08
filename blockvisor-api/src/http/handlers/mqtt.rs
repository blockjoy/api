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

use crate::auth::rbac::MqttPerm;
use crate::auth::resource::Resource;
use crate::config::Context;
use crate::database::Database;
use crate::http::response;
use crate::mqtt::handler::{self, AclRequest, Topic};

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
        use Error::*;
        error!("{self}");
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

#[allow(clippy::unused_async)]
async fn auth(WithRejection(value, _): WithRejection<Json<Value>, Error>) -> impl IntoResponse {
    debug!("MQTT auth payload: {value:?}");
    response::ok()
}

async fn acl(
    State(ctx): State<Arc<Context>>,
    WithRejection(req, _): WithRejection<Json<AclRequest>, Error>,
) -> Result<impl IntoResponse, Error> {
    let token = req.username.parse().map_err(Error::ParseRequestToken)?;
    let resource: Resource = match req.topic {
        Topic::Orgs { org_id, .. } => org_id.into(),
        Topic::Hosts { host_id, .. } => host_id.into(),
        Topic::HostStatus { host_id } => host_id.into(),
        Topic::Nodes { node_id, .. } => node_id.into(),
    };

    let mut conn = ctx.pool.conn().await?;
    let _ = ctx
        .auth
        .authorize_token(
            &token,
            MqttPerm::Acl.into(),
            Some(resource.into()),
            &mut conn,
        )
        .await?;

    Ok(response::ok())
}
