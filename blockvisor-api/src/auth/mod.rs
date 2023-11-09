pub mod claims;
pub mod endpoint;
pub mod rbac;
pub mod resource;
pub mod token;

use std::sync::Arc;

use chrono::Duration;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::Status;

use crate::config::token::Config;
use crate::database::Conn;

use self::claims::{Claims, Granted};
use self::rbac::{Perm, Perms};
use self::resource::{Resource, Resources};
use self::token::api_key::Validated;
use self::token::refresh::{self, Refresh, RequestCookie};
use self::token::{Cipher, RequestToken};

#[tonic::async_trait]
pub trait Authorize {
    /// Authorize request token for some `perms` and `resources`.
    ///
    /// This is the entry point for the authorization process which the other
    /// trait methods delegate to.
    async fn authorize(
        &mut self,
        meta: &MetadataMap,
        perms: Perms,
        resources: Option<Resources>,
    ) -> Result<AuthZ, Error>;

    /// Authorize request token for some `perms` and `resources`.
    async fn auth<P, R>(
        &mut self,
        meta: &MetadataMap,
        perms: P,
        resources: R,
    ) -> Result<AuthZ, Error>
    where
        P: Into<Perms> + Send,
        R: Into<Resources> + Send,
    {
        self.authorize(meta, perms.into(), Some(resources.into()))
            .await
    }

    /// Authorize request token for some `perms` and all resources.
    async fn auth_all<P>(&mut self, meta: &MetadataMap, perms: P) -> Result<AuthZ, Error>
    where
        P: Into<Perms> + Send,
    {
        self.authorize(meta, perms.into(), None).await
    }

    /// Try and authorize request token for `perms_all` and all resources.
    ///
    /// On failure, authorize claims for some `perms` and `resources` instead.
    async fn auth_or_all<P1, P2, R>(
        &mut self,
        meta: &MetadataMap,
        perms_all: P1,
        perms: P2,
        resources: R,
    ) -> Result<AuthZ, Error>
    where
        P1: Into<Perms> + Send,
        P2: Into<Perms> + Send,
        R: Into<Resources> + Send,
    {
        if let Ok(authz) = self.authorize(meta, perms_all.into(), None).await {
            return Ok(authz);
        }

        self.authorize(meta, perms.into(), Some(resources.into()))
            .await
    }
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth Claims error: {0}
    Claims(#[from] self::claims::Error),
    /// Database error: {0}
    Database(#[from] crate::database::Error),
    /// Failed to decode JWT: {0}
    DecodeJwt(token::jwt::Error),
    /// Failed to decode refresh BearerToken: {0}
    DecodeRefresh(refresh::Error),
    /// Failed to parse RequestToken: {0}
    ParseRequestToken(token::Error),
    /// Failed to parse refresh header: {0}
    RefreshHeader(refresh::Error),
    /// Failed to validate api key: {0}
    ValidateApiKey(token::api_key::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Database(_) => Status::internal("Internal error."),
            DecodeJwt(_) => Status::permission_denied("Invalid JWT token."),
            DecodeRefresh(_) | RefreshHeader(_) => {
                Status::permission_denied("Invalid refresh token.")
            }
            ValidateApiKey(_) => Status::permission_denied("Invalid API key."),
            Claims(err) => err.into(),
            ParseRequestToken(err) => err.into(),
        }
    }
}

pub struct Auth {
    pub cipher: Arc<Cipher>,
    pub token_expires: Duration,
}

impl Auth {
    pub fn new(config: &Config) -> Self {
        let cipher = Arc::new(Cipher::new(&config.secret));
        let token_expires = config.expire.token;

        Auth {
            cipher,
            token_expires,
        }
    }

    pub async fn authorize_metadata(
        &self,
        meta: &MetadataMap,
        perms: Perms,
        resources: Option<Resources>,
        conn: &mut Conn<'_>,
    ) -> Result<AuthZ, Error> {
        let token: RequestToken = meta.try_into().map_err(Error::ParseRequestToken)?;
        self.authorize_token(&token, perms, resources, conn).await
    }

    pub async fn authorize_token(
        &self,
        token: &RequestToken,
        perms: Perms,
        resources: Option<Resources>,
        conn: &mut Conn<'_>,
    ) -> Result<AuthZ, Error> {
        let claims = match token {
            RequestToken::Bearer(token) => self.cipher.jwt.decode(token).map_err(Error::DecodeJwt),
            RequestToken::ApiKey(token) => Validated::from_token(token, conn)
                .await
                .map_err(Error::ValidateApiKey)
                .map(|v| v.claims(self.token_expires)),
        }?;

        self.authorize_claims(claims, perms, resources, conn).await
    }

    pub async fn authorize_claims(
        &self,
        claims: Claims,
        perms: Perms,
        resources: Option<Resources>,
        conn: &mut Conn<'_>,
    ) -> Result<AuthZ, Error> {
        let initial = if let Some(resources) = resources {
            claims.ensure_resources(resources, conn).await?
        } else if let Some(user_id) = claims.resource().user() {
            Granted::from_admin(user_id, conn).await?
        } else {
            None
        };

        let granted = Granted::from_access(&claims.access, initial, conn).await?;
        match perms {
            Perms::One(perm) => granted.ensure_perm(perm, &claims)?,
            Perms::Many(perms) => granted.ensure_perms(perms, &claims)?,
        }

        Ok(AuthZ { claims, granted })
    }

    pub fn refresh(&self, meta: &MetadataMap) -> Result<Refresh, Error> {
        let cookie: RequestCookie = meta.try_into().map_err(Error::RefreshHeader)?;
        self.cipher
            .refresh
            .decode(&cookie.encoded)
            .map_err(Error::DecodeRefresh)
    }

    /// Try to get a `Refresh` token from the request headers.
    ///
    /// Will return `Ok(None)` if the header is missing so that an alternative
    /// representation may be tried (e.g. from a `gRPC` request body).
    pub fn maybe_refresh(&self, meta: &MetadataMap) -> Result<Option<Refresh>, Error> {
        use refresh::Error::*;
        match self.refresh(meta) {
            Ok(refresh) => Ok(Some(refresh)),
            Err(Error::RefreshHeader(
                MissingCookieHeader | MissingCookieRefresh | EmptyCookieRefresh,
            )) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Authorized `Claims` along with the set of `Granted` permissions.
#[derive(Debug)]
pub struct AuthZ {
    pub claims: Claims,
    pub granted: Granted,
}

impl AuthZ {
    /// Returns the authorized `Claims` resource.
    ///
    /// Note that this is not the target resource for operations.
    pub fn resource(&self) -> Resource {
        self.claims.resource()
    }

    /// Predicate to check if a specific permission is granted.
    pub fn has_perm<P>(&self, perm: P) -> bool
    where
        P: Into<Perm>,
    {
        self.granted.has_perm(perm)
    }

    /// Predicate to check if any one of the permissions are granted.
    pub fn has_any_perm<I, P>(&self, perms: I) -> bool
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm>,
    {
        self.granted.has_any_perm(perms)
    }

    /// Returns the key value from the authorized `Claims` data.
    pub fn get_data(&self, key: &str) -> Option<&str> {
        self.claims.get(key)
    }
}

impl From<&AuthZ> for Resource {
    fn from(authz: &AuthZ) -> Self {
        authz.resource()
    }
}
