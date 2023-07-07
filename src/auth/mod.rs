pub mod claims;
pub mod endpoint;
pub mod resource;
pub mod token;

use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;
use tonic::Status;
use tracing::error;

use crate::config::token::Config;
use crate::models::Conn;

use self::claims::Claims;
use self::endpoint::Endpoint;
use self::token::api_key::Validated;
use self::token::refresh::{self, Refresh, RequestCookie};
use self::token::{Cipher, RequestToken};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Claims are missing Endpoint: {0:?}
    ClaimsMissingEndpoint(Endpoint),
    /// Failed to decode JWT: {0}
    DecodeJwt(token::jwt::Error),
    /// Failed to decode refresh BearerToken: {0}
    DecodeRefresh(refresh::Error),
    /// Failed to parse RequestToken: {0}
    ParseRequestToken(token::Error),
    /// Failed to parse refresh header: {0}
    RefreshHeader(refresh::Error),
    /// Failed to parse token expiry: {0}
    TokenExpires(crate::config::Error),
    /// Failed to validate api key: {0}
    ValidateApiKey(token::api_key::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{}: {err}", std::any::type_name::<Error>());

        use Error::*;
        match err {
            ClaimsMissingEndpoint(_) => Status::permission_denied("No access to this endpoint."),
            DecodeJwt(_) => Status::permission_denied("Invalid JWT token."),
            DecodeRefresh(_) | RefreshHeader(_) => {
                Status::permission_denied("Invalid refresh token.")
            }
            ParseRequestToken(e) => e.into(),
            TokenExpires(_) => Status::internal("Internal error."),
            ValidateApiKey(_) => Status::permission_denied("Invalid API key."),
        }
    }
}

/// The entry point into the authentication process.
pub struct Auth {
    pub cipher: Arc<Cipher>,
    pub token_expires: chrono::Duration,
}

impl Auth {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let cipher = Arc::new(Cipher::new(&config.secret));
        let token_expires = config
            .expire
            .token
            .try_into()
            .map_err(Error::TokenExpires)?;

        Ok(Auth {
            cipher,
            token_expires,
        })
    }

    pub async fn claims<T>(
        &self,
        req: &tonic::Request<T>,
        endpoint: Endpoint,
        conn: &mut Conn,
    ) -> Result<Claims, Error> {
        let token: RequestToken = req
            .metadata()
            .try_into()
            .map_err(Error::ParseRequestToken)?;

        let claims = match token {
            RequestToken::ApiKey(token) => Validated::from_token(&token, conn)
                .await
                .map_err(Error::ValidateApiKey)?
                .claims(self.token_expires),

            RequestToken::Bearer(token) => {
                self.cipher.jwt.decode(&token).map_err(Error::DecodeJwt)?
            }
        };

        if !claims.endpoints.includes(endpoint) {
            return Err(Error::ClaimsMissingEndpoint(endpoint));
        }

        Ok(claims)
    }

    pub fn refresh<T>(&self, req: &tonic::Request<T>) -> Result<Refresh, Error> {
        let cookie: RequestCookie = req.metadata().try_into().map_err(Error::RefreshHeader)?;
        self.cipher
            .refresh
            .decode(&cookie.encoded)
            .map_err(Error::DecodeRefresh)
    }

    /// Try to get a `Refresh` token from the request headers.
    ///
    /// Will return `Ok(None)` if the header is missing so that an alternative
    /// representation may be tried (e.g. from a gRPC request body).
    pub fn maybe_refresh<T>(&self, req: &tonic::Request<T>) -> Result<Option<Refresh>, Error> {
        use refresh::Error::*;
        match self.refresh(req) {
            Ok(refresh) => Ok(Some(refresh)),
            Err(Error::RefreshHeader(
                MissingCookieHeader | MissingCookieRefresh | EmptyCookieRefresh,
            )) => Ok(None),
            Err(err) => Err(err),
        }
    }
}
