use crate::auth::key_provider::{KeyProvider, KeyProviderError};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use strum_macros::Display;
use thiserror::Error;

#[derive(Error, Display, Debug)]
pub enum EmqxError {
    /// Errors resulting from reading ENV vars
    EnvVar(#[from] std::env::VarError),
    /// Errors resulting from reading keys
    Key(#[from] KeyProviderError),
    /// Errors happening when deserializing data
    Deserialize(anyhow::Error),
    /// Errors happening when serializing data
    Serialize(serde_json::Error),
    /// Errors happening when calling the API
    Communication(#[from] reqwest::Error),
}

#[derive(Serialize, Display, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmqxAccessRole {
    Allow,
    Deny,
}

impl TryFrom<&str> for EmqxAccessRole {
    type Error = EmqxError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            _ => Err(EmqxError::Deserialize(anyhow!(
                "Unknown access role: {}",
                value
            ))),
        }
    }
}

#[derive(Serialize, Display, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmqxAction {
    Pub,
    Sub,
    PubSub,
}

impl TryFrom<&str> for EmqxAction {
    type Error = EmqxError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pub" => Ok(Self::Pub),
            "sub" => Ok(Self::Sub),
            "pubsub" => Ok(Self::PubSub),
            _ => Err(EmqxError::Deserialize(anyhow!("Unknown action: {}", value))),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct EmqxPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub topic: String,
    pub action: EmqxAction,
    pub access: EmqxAccessRole,
}

#[derive(Deserialize)]
pub struct EmqxRuleObject {
    topic: String,
    result: String,
    client_id: Option<String>,
    username: Option<String>,
    action: String,
    access: String,
}

#[derive(Deserialize)]
pub struct EmqxResponse {
    code: i32,
    data: EmqxRuleObject,
}

pub type EmqxResult<T> = Result<T, EmqxError>;

#[derive(Debug)]
pub struct EmqxApi {
    base_url: String,
    app_id: String,
    app_secret: String,
    client: reqwest::Client,
}

impl std::fmt::Display for EmqxApi {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.base_url)
    }
}

impl EmqxApi {
    pub fn new() -> EmqxResult<Self> {
        let base_url = std::env::var("EMQX_BASE_URL")?;
        let app_id = KeyProvider::get_var("EMQX_APP_ID")?.value();
        let app_secret = KeyProvider::get_var("EMQX_SECRET")?.value();
        let client = reqwest::Client::new();

        Ok(Self {
            base_url,
            app_id,
            app_secret,
            client,
        })
    }

    pub fn base_url(&self) -> String {
        self.build_url("/")
    }

    pub async fn add_user_acl(
        &self,
        user_id: String,
        topic: String,
        action: EmqxAction,
        access: EmqxAccessRole,
    ) -> EmqxResult<EmqxResponse> {
        let payload = EmqxPayload {
            client_id: None,
            user_id: Some(user_id),
            topic,
            action,
            access,
        };

        self.add_acl(payload).await
    }

    async fn add_acl(&self, payload: EmqxPayload) -> EmqxResult<EmqxResponse> {
        let payload =
            serde_json::to_string::<EmqxPayload>(&payload).map_err(EmqxError::Serialize)?;
        let response = self
            .client
            .post(&self.build_url("acl"))
            .json(&payload)
            .send()
            .await?;
        let response = response.json::<EmqxResponse>().await?;

        Ok(response)
    }

    fn build_url(&self, endpoint: &str) -> String {
        format!("{}/{}", &self.base_url, endpoint)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::key_provider::KeyProvider;
    use crate::emqx_api::{EmqxAccessRole, EmqxAction, EmqxError, EmqxPayload, EmqxResponse};
    use http::StatusCode;

    #[test]
    fn can_serialize_user_payload() -> anyhow::Result<()> {
        let payload = EmqxPayload {
            client_id: None,
            user_id: Some("user-2".to_string()),
            topic: "foo-bar".to_string(),
            action: EmqxAction::Pub,
            access: EmqxAccessRole::Allow,
        };
        let json = serde_json::to_string::<EmqxPayload>(&payload).map_err(EmqxError::Serialize)?;
        let expected = r#"{"user_id":"user-2","topic":"foo-bar","action":"pub","access":"allow"}"#;

        assert_eq!(json, expected);

        Ok(())
    }

    #[test]
    fn can_serialize_client_payload() -> anyhow::Result<()> {
        let payload = EmqxPayload {
            client_id: Some("client-2".to_string()),
            user_id: None,
            topic: "foo-bar".to_string(),
            action: EmqxAction::Pub,
            access: EmqxAccessRole::Allow,
        };
        let json = serde_json::to_string::<EmqxPayload>(&payload).map_err(EmqxError::Serialize)?;
        let expected =
            r#"{"client_id":"client-2","topic":"foo-bar","action":"pub","access":"allow"}"#;

        assert_eq!(json, expected);

        Ok(())
    }

    #[test]
    fn can_deserialize_response() -> anyhow::Result<()> {
        let json = r#"
            {
              "data": {
                "username": "user1", 
                "topic": "foo-bar", 
                "result": "ok", 
                "action": "pubsub", 
                "access": "allow"
              }, 
              "code": 123
            }
        "#;

        let response = serde_json::from_str::<EmqxResponse>(json)?;

        assert_eq!(response.data.username.unwrap(), "user1");
        assert_eq!(response.data.topic, "foo-bar");
        assert_eq!(response.data.result, "ok");
        assert_eq!(
            EmqxAction::try_from(response.data.action.as_str())?,
            EmqxAction::PubSub
        );
        assert_eq!(
            EmqxAccessRole::try_from(response.data.access.as_str())?,
            EmqxAccessRole::Allow
        );
        assert_eq!(response.code, 123);

        Ok(())
    }

    #[tokio::test]
    async fn can_call_api() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let url = format!("{}/acl/clientid", std::env::var("EMQX_BASE_URL")?);
        let app_id = KeyProvider::get_var("EMQX_APP_ID")?.value();
        let app_secret = KeyProvider::get_var("EMQX_SECRET")?.value();
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .basic_auth(app_id, Some(app_secret))
            .send()
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        Ok(())
    }
}
