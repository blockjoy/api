use std::time::Duration;

use displaydoc::Display;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use thiserror::Error;
use url::Url;

use crate::config::Redacted;

use super::api::{ApiErrors, ApiSuccess, Endpoint};

const API_URL: &str = "https://api.cloudflare.com/client/v4/";
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);
const CONTENT_JSON: &str = "application/json";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build cloudflare Client: {0}
    BuildClient(reqwest::Error),
    /// Failed to get response from cloudflare: {0}
    GetResponse(reqwest::Error),
    /// Failed to join cloudflare endpoint url: {0}
    JoinEndpoint(url::ParseError),
    /// Failed to parse cloudflare API endpoint: {0}
    ParseEndpoint(url::ParseError),
    /// Failed to parse cloudflare response errors: {0}
    ParseErrors(reqwest::Error),
    /// Failed to parse cloudflare response with error `{0}`. Response body was: {1}
    ParseResponse(serde_json::Error, String),
    /// Error code {0} from cloudflare: {1:?}
    ResponseErrors(reqwest::StatusCode, ApiErrors),
    /// Failed to send cloudflare request: {0}
    SendRequest(reqwest::Error),
}

pub struct Client {
    inner: reqwest::Client,
    endpoint: Url,
    bearer: Redacted<String>,
}

impl Client {
    pub fn new(token: &str) -> Result<Client, Error> {
        let inner = reqwest::Client::builder()
            .timeout(CLIENT_TIMEOUT)
            .build()
            .map_err(Error::BuildClient)?;
        let endpoint = API_URL.parse().map_err(Error::ParseEndpoint)?;
        let bearer = format!("Bearer {token}").into();

        Ok(Client {
            inner,
            endpoint,
            bearer,
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mock(endpoint: Url) -> Result<Self, Error> {
        let inner = reqwest::Client::builder()
            .timeout(CLIENT_TIMEOUT)
            .build()
            .map_err(Error::BuildClient)?;
        let bearer = "Bearer mock-token".to_string().into();

        Ok(Client {
            inner,
            endpoint,
            bearer,
        })
    }

    pub async fn request<E>(&self, endpoint: &E) -> Result<E::Result, Error>
    where
        E: Endpoint,
    {
        let url = self
            .endpoint
            .join(&endpoint.path())
            .map_err(Error::JoinEndpoint)?;

        let mut request = self.inner.request(endpoint.method(), url);
        request = request.header(AUTHORIZATION, &*self.bearer);

        if let Some(body) = endpoint.body() {
            request = request.body(body);
            request = request.header(CONTENT_TYPE, CONTENT_JSON);
        }

        let response = request.send().await.map_err(Error::SendRequest)?;
        let status = response.status();

        if status.is_success() {
            let resp_text = response.text().await.map_err(Error::GetResponse)?;

            let success: ApiSuccess<_> =
                serde_json::from_str(&resp_text).map_err(|e| Error::ParseResponse(e, resp_text))?;
            Ok(success.result)
        } else {
            let errors: ApiErrors = response.json().await.map_err(Error::ParseErrors)?;
            Err(Error::ResponseErrors(status, errors))
        }
    }
}
