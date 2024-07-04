use std::time::Duration;

use displaydoc::Display;
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;
use thiserror::Error;
use url::Url;

use super::api::StripeEndpoint;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const CONTENT_FORM_ENCODED: &str = "application/x-www-form-urlencoded";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build stripe Client: {0}
    BuildClient(reqwest::Error),
    /// Failed to join stripe endpoint url: {0}
    JoinEndpoint(url::ParseError),
    /// Failed to parse stripe API endpoint: {0}
    ParseEndpoint(url::ParseError),
    /// Failed to parse stripe response with code `{0}`: {1}
    ParseResponse(reqwest::StatusCode, reqwest::Error),
    /// Error code {0} from stripe: {1}
    ResponseCode(reqwest::StatusCode, String),
    /// Failed to send stripe request: {0}
    SendRequest(reqwest::Error),
}

pub struct Client {
    inner: reqwest::Client,
    endpoint: Url,
    secret: String,
}

impl Client {
    pub fn new(secret: &str, base_url: &str) -> Result<Client, Error> {
        let inner = reqwest::Client::builder()
            .timeout(CLIENT_TIMEOUT)
            .build()
            .map_err(Error::BuildClient)?;
        let endpoint = base_url.parse().map_err(Error::ParseEndpoint)?;

        Ok(Client {
            inner,
            endpoint,
            secret: secret.to_owned(),
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mock(endpoint: Url) -> Result<Self, Error> {
        let inner = reqwest::Client::builder()
            .timeout(CLIENT_TIMEOUT)
            .build()
            .map_err(Error::BuildClient)?;
        let secret = "stripe_fake_secret".to_string();

        Ok(Client {
            inner,
            endpoint,
            secret,
        })
    }

    pub async fn request<E>(&self, endpoint: &E) -> Result<E::Result, Error>
    where
        E: StripeEndpoint + Serialize + std::fmt::Debug,
    {
        let url = dbg!(dbg!(&self.endpoint)
            .join(&endpoint.path())
            .map_err(Error::JoinEndpoint)?);

        let mut request = self
            .inner
            .request(endpoint.method(), url)
            .basic_auth(&self.secret, None as Option<String>);

        if let Some(body) = endpoint.body() {
            request = request.form(body);
            request = request.header(CONTENT_TYPE, CONTENT_FORM_ENCODED);
        }

        if let Some(query) = endpoint.query() {
            request = request.query(query);
        }

        let resp = dbg!(dbg!(request).send().await).map_err(Error::SendRequest)?;
        let status = resp.status();
        if status.is_success() {
            resp.json()
                .await
                .map_err(|err| Error::ParseResponse(status, err))
        } else {
            let message = resp
                .text()
                .await
                .map_err(|err| Error::ParseResponse(status, err))?;
            Err(Error::ResponseCode(status, message))
        }
    }
}
