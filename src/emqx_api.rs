use crate::auth::key_provider::{KeyProvider, KeyProviderError};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use strum_macros::Display;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmqxError {
    /// Errors resulting from reading ENV vars
    #[error("Error reading env var: {0}")]
    EnvVar(#[from] std::env::VarError),
    /// Errors resulting from reading keys
    #[error("Error reading key: {0}")]
    Key(#[from] KeyProviderError),
    /// Errors happening when deserializing data
    #[error("Error deserializing JSON: {0}")]
    Deserialize(anyhow::Error),
    /// Errors happening when serializing data
    #[error("Error serializing JSON: {0}")]
    Serialize(serde_json::Error),
    /// Errors happening when calling the API
    #[error("Error communicating with API: {0}")]
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
#[serde(rename_all = "lowercase")]
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
    #[serde(rename(serialize = "clientid"))]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename(serialize = "username", deserialize = "username"))]
    pub user_id: Option<String>,
    pub topic: String,
    pub action: EmqxAction,
    pub access: EmqxAccessRole,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EmqxRuleObject {
    pub topic: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename(deserialize = "clientid"))]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub action: String,
    pub access: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EmqxUserPayload {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Debug)]
pub struct EmqxResponse {
    pub code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<EmqxRuleObject>,
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
        let base_url = KeyProvider::get_var("EMQX_BASE_URL")?.value();
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

    pub async fn remove_user_acl(
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

        self.remove_acl(payload).await
    }

    pub async fn add_client_acl(
        &self,
        client_id: String,
        topic: String,
        action: EmqxAction,
        access: EmqxAccessRole,
    ) -> EmqxResult<EmqxResponse> {
        let payload = EmqxPayload {
            client_id: Some(client_id),
            user_id: None,
            topic,
            action,
            access,
        };

        self.add_acl(payload).await
    }

    pub async fn remove_client_acl(
        &self,
        client_id: String,
        topic: String,
        action: EmqxAction,
        access: EmqxAccessRole,
    ) -> EmqxResult<EmqxResponse> {
        let payload = EmqxPayload {
            client_id: Some(client_id),
            user_id: None,
            topic,
            action,
            access,
        };

        self.remove_acl(payload).await
    }

    pub async fn create_username(&self, payload: EmqxUserPayload) -> EmqxResult<EmqxResponse> {
        let response = self
            .client
            .post(&self.build_url("auth_username"))
            .basic_auth(&self.app_id, Some(&self.app_secret))
            .json(&payload)
            .send()
            .await?;
        let response = response.json::<EmqxResponse>().await?;

        Ok(response)
    }

    async fn add_acl(&self, payload: EmqxPayload) -> EmqxResult<EmqxResponse> {
        let response = self
            .client
            .post(&self.build_url("acl"))
            .basic_auth(&self.app_id, Some(&self.app_secret))
            .json(&payload)
            .send()
            .await?;
        let response = response.json::<EmqxResponse>().await?;

        Ok(response)
    }

    async fn remove_acl(&self, payload: EmqxPayload) -> EmqxResult<EmqxResponse> {
        let (resource, value, topic) = if payload.client_id.clone().is_some() {
            (
                "clientid",
                payload.client_id.clone().unwrap(),
                payload.topic.clone(),
            )
        } else {
            (
                "username",
                payload.user_id.clone().unwrap(),
                payload.topic.clone(),
            )
        };
        let response = self
            .client
            .delete(&self.build_url(format!("acl/{}/{}/topic/{}", resource, value, topic).as_str()))
            .basic_auth(&self.app_id, Some(&self.app_secret))
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
    use crate::emqx_api::{
        EmqxAccessRole, EmqxAction, EmqxApi, EmqxError, EmqxPayload, EmqxResponse, EmqxUserPayload,
    };
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
        let expected = r#"{"username":"user-2","topic":"foo-bar","action":"pub","access":"allow"}"#;

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
            r#"{"clientid":"client-2","topic":"foo-bar","action":"pub","access":"allow"}"#;

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
        let data = response.data.unwrap();

        assert_eq!(data.username.unwrap(), "user1");
        assert_eq!(data.topic, "foo-bar");
        assert_eq!(data.result, "ok");
        assert_eq!(
            EmqxAction::try_from(data.action.as_str())?,
            EmqxAction::PubSub
        );
        assert_eq!(
            EmqxAccessRole::try_from(data.access.as_str())?,
            EmqxAccessRole::Allow
        );
        assert_eq!(response.code, 123);

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn can_call_api() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let url = format!(
            "{}/acl/clientid",
            KeyProvider::get_var("EMQX_BASE_URL")?.value()
        );
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

    #[tokio::test]
    #[ignore]
    async fn can_create_client_acl() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let api = EmqxApi::new()?;
        let response = api
            .add_client_acl(
                "stribu".to_string(),
                "feed-me".to_string(),
                EmqxAction::Pub,
                EmqxAccessRole::Allow,
            )
            .await?;

        assert_eq!(response.code, 0);
        assert_eq!(
            response.data.unwrap().client_id.unwrap(),
            "stribu".to_string()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn can_create_user_acl() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let api = EmqxApi::new()?;
        let response = api
            .add_user_acl(
                "Strizzi".to_string(),
                "feed-me-more".to_string(),
                EmqxAction::PubSub,
                EmqxAccessRole::Allow,
            )
            .await?;

        assert_eq!(response.code, 0);
        assert_eq!(
            response.data.unwrap().username.unwrap(),
            "Strizzi".to_string()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn can_remove_client_acl() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let api = EmqxApi::new()?;
        let response = api
            .remove_client_acl(
                "stribu".to_string(),
                "feed-me".to_string(),
                EmqxAction::Pub,
                EmqxAccessRole::Allow,
            )
            .await?;

        assert_eq!(response.code, 0);

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn can_remove_user_acl() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let api = EmqxApi::new()?;
        let response = api
            .remove_user_acl(
                "Strizzi".to_string(),
                "feed-me-more".to_string(),
                EmqxAction::PubSub,
                EmqxAccessRole::Allow,
            )
            .await?;

        assert_eq!(response.code, 0);

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn can_create_username() -> anyhow::Result<()> {
        dotenv::dotenv().ok();

        let api = EmqxApi::new()?;
        let payload = EmqxUserPayload {
            username: uuid::Uuid::new_v4().to_string(),
            password: "pwd123".to_string(),
        };
        let response = api.create_username(payload).await?;

        assert_eq!(response.code, 0);

        Ok(())
    }
}
