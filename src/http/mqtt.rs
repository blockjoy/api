use crate::auth::{HostAuthToken, UserAuthToken};
use anyhow::anyhow;
use serde::Deserialize;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MqttPolicyError {
    #[error("Unknown MQTT policy error: {0}")]
    Unknown(anyhow::Error),
    #[error("Error validating token: {0}")]
    Token(#[from] crate::auth::token::TokenError),
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

pub trait MqttAclPolicy {
    fn allow(token: &str, topic: String) -> MqttAclPolicyResult;
}

pub struct MqttUserPolicy;
pub struct MqttHostPolicy;

#[tonic::async_trait]
impl MqttAclPolicy for MqttUserPolicy {
    /// TODO
    fn allow(token: &str, _topic: String) -> MqttAclPolicyResult {
        // Verify token
        let token = UserAuthToken::from_str(token)?;
        let _org_id = token.data.get("org_id").unwrap_or(&String::new());

        tracing::info!("MqttUserPolicy returns true");

        Ok(true)
    }
}

impl MqttAclPolicy for MqttHostPolicy {
    fn allow(token: &str, topic: String) -> MqttAclPolicyResult {
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
