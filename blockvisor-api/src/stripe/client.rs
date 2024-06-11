use std::time::Duration;

use displaydoc::Display;
use reqwest::header::CONTENT_TYPE;
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
    /// Failed to parse stripe response: {0}
    ParseResponse(reqwest::Error),
    /// Bad error code from stripe: {0}
    ResponseCode(reqwest::Error),
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
        E: StripeEndpoint,
    {
        let url = self
            .endpoint
            .join(&endpoint.path())
            .map_err(Error::JoinEndpoint)?;

        let mut request = self
            .inner
            .request(endpoint.method(), url)
            .basic_auth(&self.secret, None as Option<String>);

        if let Some(body) = endpoint.body() {
            println!("This is the body:");
            println!("{body}");
            request = request.body(body);
            request = request.header(CONTENT_TYPE, CONTENT_FORM_ENCODED);
        }

        let resp = request
            .send()
            .await
            .map_err(Error::SendRequest)?
            .error_for_status()
            .map_err(Error::ResponseCode)?
            .json()
            .await
            .map_err(Error::ParseResponse)?;
        Ok(resp)
    }
}
