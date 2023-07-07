use std::str::FromStr;

use anyhow::anyhow;
use serde::Deserialize;

use crate::auth::claims::Claims;
use crate::auth::resource::{HostId, NodeId, OrgId};
use crate::models::Conn;

/// This is a list of our supported MQTT topics.
pub enum Topic {
    /// `/orgs/<uuid>/...`
    Orgs { org_id: OrgId, rest: String },
    /// `/host/<uuid>/...`
    Hosts { host_id: HostId, rest: String },
    /// `/nodes/<uuid>/...`
    Nodes { node_id: NodeId, rest: String },
}

impl FromStr for Topic {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("/orgs/") {
            let org_id = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too short to contain a valid uuid"))?
                .parse()?;
            Ok(Self::Orgs {
                org_id,
                rest: rest.get(37..).unwrap_or_default().to_owned(),
            })
        } else if let Some(rest) = s.strip_prefix("/hosts/") {
            let host_id = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too short to contain a valid uuid"))?
                .parse()?;
            Ok(Self::Hosts {
                host_id,
                rest: rest.get(37..).unwrap_or_default().to_owned(),
            })
        } else if let Some(rest) = s.strip_prefix("/nodes/") {
            let node_id = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too short to contain a valid uuid"))?
                .parse()?;
            Ok(Self::Nodes {
                node_id,
                rest: rest.get(37..).unwrap_or_default().to_owned(),
            })
        } else {
            Err(anyhow!("Unparsable topic").into())
        }
    }
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Publish,
    Subscribe,
}

#[derive(Deserialize, Debug)]
pub struct AclRequest {
    pub operation: String,
    pub username: String,
    pub topic: String,
}

#[derive(Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
}

pub async fn allow(claims: Claims, topic: Topic, conn: &mut Conn) -> crate::Result<bool> {
    use crate::auth::claims::Error::*;

    match topic {
        Topic::Orgs { org_id, .. } => match claims.ensure_org(org_id, false, conn).await {
            Ok(_claims) => Ok(true),
            Err(EnsureOrg) => Ok(false),
            Err(err) => Err(err.into()),
        },

        Topic::Hosts { host_id, .. } => match claims.ensure_host(host_id, false, conn).await {
            Ok(_claims) => Ok(true),
            Err(EnsureHost) => Ok(false),
            Err(err) => Err(err.into()),
        },

        Topic::Nodes { node_id, .. } => match claims.ensure_node(node_id, false, conn).await {
            Ok(_claims) => Ok(true),
            Err(EnsureNode) => Ok(false),
            Err(err) => Err(err.into()),
        },
    }
}
