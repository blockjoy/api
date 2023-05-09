use crate::{auth, models};
use anyhow::{anyhow, Context};
use serde::Deserialize;

/// This is a list of our supported MQTT topics.
pub enum MqttTopic {
    /// `/orgs/<uuid>/...`
    Orgs { org_id: uuid::Uuid, rest: String },
    /// `/nodes/<uuid>/...`
    Nodes { node_id: uuid::Uuid, rest: String },
}

impl std::str::FromStr for MqttTopic {
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
pub enum MqttOperationType {
    Publish,
    Subscribe,
}

#[derive(Deserialize, Debug)]
pub struct MqttAclRequest {
    pub operation: String,
    pub username: String,
    pub topic: String,
}

#[derive(Deserialize)]
pub struct MqttAuthRequest {
    pub username: String,
    pub password: String,
}

pub struct MqttPolicy {
    pub db: models::DbPool,
}

impl MqttPolicy {
    pub async fn allow(&self, token: auth::Jwt, topic: &str) -> crate::Result<bool> {
        let topic: MqttTopic = topic.parse()?;
        let mut conn = self
            .db
            .conn()
            .await
            .with_context(|| "Couldn't get database connection")?;
        let is_allowed = match (token.claims.resource(), topic) {
            // A user is allowed to listen for updates on an org channel if they're a member of that
            // org
            (auth::Resource::User(user_id), MqttTopic::Orgs { org_id, .. }) => {
                models::Org::is_member(user_id, org_id, &mut conn).await?
            }
            // A user is allowed to listen for updates on a node channel if that node belongs to the
            // same org as them
            (auth::Resource::User(user_id), MqttTopic::Nodes { node_id, .. }) => {
                let node = models::Node::find_by_id(node_id, &mut conn).await?;
                models::Org::is_member(user_id, node.org_id, &mut conn).await?
            }
            // An org is allowed to listen for updates on an org channel if that org is the same as
            // them.
            (auth::Resource::Org(org_id_), MqttTopic::Orgs { org_id, .. }) => org_id == org_id_,
            // An org is allowed to listen for updates on a node channel if that nodes belongs to
            // them.
            (auth::Resource::Org(org_id), MqttTopic::Nodes { node_id, .. }) => {
                let node = models::Node::find_by_id(node_id, &mut conn).await?;
                node.org_id == org_id
            }
            // A host is not allowed to listen to the messages about a whole org.
            (auth::Resource::Host(_), MqttTopic::Orgs { .. }) => false,
            // A host is allowed to listen to the messages for a node if that node is running on
            // them.
            (auth::Resource::Host(host_id), MqttTopic::Nodes { node_id, .. }) => {
                let node = models::Node::find_by_id(node_id, &mut conn).await?;
                node.host_id == host_id
            }
            // A node is not allowed to listen for messages about a whole org.
            (auth::Resource::Node(_), MqttTopic::Orgs { .. }) => false,
            // A node is allowed to listen for messages about a node if that node is them.
            (auth::Resource::Node(node_id_), MqttTopic::Nodes { node_id, .. }) => {
                node_id_ == node_id
            }
        };
        Ok(is_allowed)
    }
}
