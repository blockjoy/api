use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, Router};
use displaydoc::Display;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, error};

use crate::auth::claims::Claims;
use crate::auth::resource::{HostId, NodeId, OrgId};
use crate::config::Context;
use crate::database::{Conn, Database};
use crate::http::response;

const UUID_LEN: usize = 36;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Database error: {0}
    Database(#[from] crate::database::Error),
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse HostId: {0}
    ParseNodeId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse RequestToken: {0}
    ParseRequestToken(crate::auth::token::Error),
    /// Topic does not contain a valid UUID.
    TopicLen,
    /// Unknown Topic type: {0}
    UnknownTopic(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        error!("{}: {self}", std::any::type_name::<Error>());

        use Error::*;
        match self {
            Auth(_) | Claims(_) => response::unauthorized().into_response(),
            Database(_) => response::failed().into_response(),
            ParseHostId(_) | ParseNodeId(_) | ParseOrgId(_) | ParseRequestToken(_) | TopicLen
            | UnknownTopic(_) => response::bad_params().into_response(),
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

async fn auth(Json(value): Json<Value>) -> impl IntoResponse {
    debug!("MQTT auth payload: {value:?}");
    response::ok()
}

async fn acl(
    State(ctx): State<Arc<Context>>,
    Json(req): Json<AclRequest>,
) -> Result<impl IntoResponse, Error> {
    let mut conn = ctx.pool.conn().await?;

    let token = req.username.parse().map_err(Error::ParseRequestToken)?;
    let claims = ctx.auth.claims_from_token(&token, &mut conn).await?;

    req.allow(claims, &mut conn).await?;

    Ok(response::ok())
}

#[derive(Debug, Deserialize)]
struct AclRequest {
    #[allow(dead_code)]
    operation: OperationType,
    username: String,
    topic: Topic,
}

impl AclRequest {
    async fn allow(&self, claims: Claims, conn: &mut Conn<'_>) -> Result<(), Error> {
        match self.topic {
            Topic::Orgs { org_id, .. } => claims
                .ensure_org(org_id, false, conn)
                .await
                .map(|_claims| ())
                .map_err(Into::into),

            Topic::Hosts { host_id, .. } => claims
                .ensure_host(host_id, false, conn)
                .await
                .map(|_claims| ())
                .map_err(Into::into),

            Topic::Nodes { node_id, .. } => claims
                .ensure_node(node_id, false, conn)
                .await
                .map(|_claims| ())
                .map_err(Into::into),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OperationType {
    Publish,
    Subscribe,
}

/// This is a list of our supported MQTT topics.
#[derive(Debug)]
#[allow(dead_code)]
enum Topic {
    /// `/orgs/<uuid>/...`
    Orgs { org_id: OrgId, rest: String },
    /// `/hosts/<uuid>/...`
    Hosts { host_id: HostId, rest: String },
    /// `/nodes/<uuid>/...`
    Nodes { node_id: NodeId, rest: String },
}

impl FromStr for Topic {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (topic, suffix) = if let Some(suffix) = s.strip_prefix("/orgs/") {
            Ok((TopicType::Orgs, suffix))
        } else if let Some(suffix) = s.strip_prefix("/hosts/") {
            Ok((TopicType::Hosts, suffix))
        } else if let Some(suffix) = s.strip_prefix("/nodes/") {
            Ok((TopicType::Nodes, suffix))
        } else {
            Err(Error::UnknownTopic(s.into()))
        }?;

        let (uuid, rest) = if suffix.len() < UUID_LEN {
            Err(Error::TopicLen)
        } else {
            Ok(suffix.split_at(UUID_LEN))
        }?;

        match topic {
            TopicType::Orgs => Ok(Topic::Orgs {
                org_id: uuid.parse().map_err(Error::ParseOrgId)?,
                rest: rest.to_string(),
            }),
            TopicType::Hosts => Ok(Topic::Hosts {
                host_id: uuid.parse().map_err(Error::ParseHostId)?,
                rest: rest.to_string(),
            }),
            TopicType::Nodes => Ok(Topic::Nodes {
                node_id: uuid.parse().map_err(Error::ParseNodeId)?,
                rest: rest.to_string(),
            }),
        }
    }
}

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).and_then(|s| s.parse().map_err(serde::de::Error::custom))
    }
}

#[derive(Clone, Copy)]
enum TopicType {
    Orgs,
    Hosts,
    Nodes,
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn parse_topic() {
        let uuid = Uuid::new_v4().to_string();
        let tests = [
            (format!("/org/{uuid}"), false),
            (format!("orgs/{uuid}"), false),
            (format!("/orgs/{uuid}"), true),
            (format!("/orgs/{uuid}/"), true),
            (format!("/orgs/{uuid}/stuff"), true),
            (format!("/hosts/{uuid}/"), true),
            (format!("/nodes/{uuid}/"), true),
        ];

        for (test, pass) in tests {
            let result = test.parse::<Topic>();
            if pass {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }
}
