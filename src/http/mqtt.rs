use crate::{
    auth::{FindableById, HostAuthToken, UserAuthToken},
    models,
};
use anyhow::{anyhow, Context};
use serde::Deserialize;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MqttPolicyError {
    #[error("Unknown MQTT policy error: {0}")]
    Unknown(#[from] anyhow::Error),
    #[error("Error validating token: {0}")]
    Token(#[from] crate::auth::token::TokenError),
    #[error("Error parsing uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("Can't use topic: {0}")]
    Topic(anyhow::Error),
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

pub type MqttAclPolicyResult = Result<bool, MqttPolicyError>;

#[tonic::async_trait]
pub trait MqttAclPolicy {
    async fn allow(&self, token: &str, topic: String) -> MqttAclPolicyResult;
}

pub struct MqttUserPolicy {
    pub db: models::DbPool,
}

pub struct MqttHostPolicy;

#[tonic::async_trait]
impl MqttAclPolicy for MqttUserPolicy {
    async fn allow(&self, token: &str, topic: String) -> MqttAclPolicyResult {
        // Verify token
        let data = UserAuthToken::from_str(token)?.data;

        let is_allowed = if let Some(rest) = topic.strip_prefix("/nodes/") {
            // If we are subscribing to an org-specific topic, we need
            let org_id = data
                .get("org_id")
                .ok_or_else(|| anyhow!("token.org_id is required"))?
                .parse()?;
            let node_id = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too shoprt to contain a valid uuid"))?
                .parse()?;
            let mut conn = self
                .db
                .conn()
                .await
                .with_context(|| "Couldn't get database connection")?;
            let node = models::Node::find_by_id(node_id, &mut conn)
                .await
                .with_context(|| "No such node")?;

            node.org_id == org_id
        } else {
            false
        };

        tracing::info!("MqttUserPolicy returns {is_allowed}");

        Ok(is_allowed)
    }
}

#[tonic::async_trait]
impl MqttAclPolicy for MqttHostPolicy {
    async fn allow(&self, token: &str, topic: String) -> MqttAclPolicyResult {
        let token = HostAuthToken::from_str(token)?;
        let host_id = topic
            .split('/')
            .nth(3)
            .ok_or("")
            .map_err(|e| MqttPolicyError::Topic(anyhow!(e)))?;
        let result = token.id.to_string().as_str() == host_id;

        tracing::info!("MqttAclPolicy returns: {result}");

        Ok(result)
    }
}
