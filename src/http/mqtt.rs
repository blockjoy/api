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

pub type MqttAclPolicyResult<T = bool> = Result<T, MqttPolicyError>;

#[tonic::async_trait]
pub trait MqttAclPolicy {
    async fn allow(&self, token: &str, topic: &str) -> MqttAclPolicyResult;
}

pub struct MqttUserPolicy {
    pub db: models::DbPool,
}

pub struct MqttHostPolicy;

#[tonic::async_trait]
impl MqttAclPolicy for MqttUserPolicy {
    async fn allow(&self, token: &str, topic: &str) -> MqttAclPolicyResult {
        // Verify token
        let token = UserAuthToken::from_str(token)?;
        let user_org_id = token.try_org_id().with_context(|| "Policy error")?;
        let is_allowed = if let Some(rest) = topic.strip_prefix("/orgs/") {
            let org_id: uuid::Uuid = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too short to contain a valid uuid"))?
                .parse()?;
            org_id == user_org_id
        } else if let Some(rest) = topic.strip_prefix("/nodes/") {
            let node_id = rest
                .get(..36)
                .ok_or_else(|| anyhow!("`{rest}` is too short to contain a valid uuid"))?
                .parse()?;
            let mut conn = self
                .db
                .conn()
                .await
                .with_context(|| "Couldn't get database connection")?;
            let node = models::Node::find_by_id(node_id, &mut conn)
                .await
                .with_context(|| "No such node")?;
            node.org_id == user_org_id
        } else {
            false
        };

        tracing::info!("MqttUserPolicy returns {is_allowed}");

        Ok(is_allowed)
    }
}

#[tonic::async_trait]
impl MqttAclPolicy for MqttHostPolicy {
    async fn allow(&self, token: &str, topic: &str) -> MqttAclPolicyResult {
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
