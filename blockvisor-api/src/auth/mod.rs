pub mod claims;
pub mod rbac;
pub mod resource;
pub mod token;

use std::sync::Arc;

use chrono::Duration;
use displaydoc::Display;
use thiserror::Error;

use crate::config::token::Config;
use crate::database::Conn;
use crate::grpc::{Metadata, Status};

use self::claims::{Claims, Granted};
use self::rbac::{Perm, Perms};
use self::resource::{Resource, Resources};
use self::token::api_key::Validated;
use self::token::refresh::{self, Refresh, RequestCookie};
use self::token::{Cipher, RequestToken};

/// The exact string for clients to match on to handle expired tokens.
const TOKEN_EXPIRED: &str = "TOKEN_EXPIRED";

pub(crate) trait Authorize {
    /// Authorize request token for `perms` and optionally ensure `resources`.
    ///
    /// This is the `AuthZ` entry point which other trait methods delegate to.
    async fn authorize(
        &mut self,
        meta: &Metadata,
        perms: Perms,
        resources: Option<Resources>,
    ) -> Result<AuthZ, Error>;

    /// Authorize request token for `perms` and all resources.
    async fn auth<P>(&mut self, meta: &Metadata, perms: P) -> Result<AuthZ, Error>
    where
        P: Into<Perms> + Send,
    {
        self.authorize(meta, perms.into(), None).await
    }

    /// Authorize request token for `perms` and ensure `resources`.
    async fn auth_for<P, R>(
        &mut self,
        meta: &Metadata,
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

    /// Try and authorize request token for `admin_perms` and all resources.
    ///
    /// Otherwise authorize claims for `user_perms` and ensure `user_resources`.
    async fn auth_or_for<AP, UP, UR>(
        &mut self,
        meta: &Metadata,
        admin_perms: AP,
        user_perms: UP,
        user_resources: UR,
    ) -> Result<AuthZ, Error>
    where
        AP: Into<Perms> + Send,
        UP: Into<Perms> + Send,
        UR: Into<Resources> + Send,
    {
        if let Ok(authz) = self.authorize(meta, admin_perms.into(), None).await {
            return Ok(authz);
        }

        self.authorize(meta, user_perms.into(), Some(user_resources.into()))
            .await
    }

    /// Authorize request token for all `perms` and all resources.
    #[allow(unused)]
    async fn auth_all<I, P>(&mut self, meta: &Metadata, perms: I) -> Result<AuthZ, Error>
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm> + Send,
    {
        let perms = Perms::All(perms.into_iter().map(Into::into).collect());
        self.authorize(meta, perms, None).await
    }

    /// Authorize request token for all `perms` and ensure `resources`.
    #[allow(unused)]
    async fn auth_all_for<I, P, R>(
        &mut self,
        meta: &Metadata,
        perms: I,
        resources: R,
    ) -> Result<AuthZ, Error>
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm> + Send,
        R: Into<Resources> + Send,
    {
        let perms = Perms::All(perms.into_iter().map(Into::into).collect());
        self.authorize(meta, perms, Some(resources.into())).await
    }

    /// Authorize request token for any `perms` and all resources.
    async fn auth_any<I, P>(&mut self, meta: &Metadata, perms: I) -> Result<AuthZ, Error>
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm> + Send,
    {
        let perms = Perms::Any(perms.into_iter().map(Into::into).collect());
        self.authorize(meta, perms, None).await
    }

    /// Authorize request token for any `perms` and ensure `resources`.
    #[allow(unused)]
    async fn auth_any_for<I, P, R>(
        &mut self,
        meta: &Metadata,
        perms: I,
        resources: R,
    ) -> Result<AuthZ, Error>
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm> + Send,
        R: Into<Resources> + Send,
    {
        let perms = Perms::Any(perms.into_iter().map(Into::into).collect());
        self.authorize(meta, perms, Some(resources.into())).await
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
    /// JWT for resource {0} has expired.
    ExpiredJwt(String),
    /// Refresh token for resource {0} has expired.
    ExpiredRefresh(String),
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
            DecodeJwt(_) => Status::forbidden("Invalid JWT token."),
            DecodeRefresh(_) | RefreshHeader(_) => Status::forbidden("Invalid refresh token."),
            ExpiredJwt(_) => Status::unauthorized(TOKEN_EXPIRED),
            ExpiredRefresh(_) => Status::unauthorized(TOKEN_EXPIRED),
            ValidateApiKey(_) => Status::forbidden("Invalid API key."),
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
        meta: &Metadata,
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
        let claims = self.claims(token, conn).await?;
        self.authorize_claims(claims, perms, resources, conn).await
    }

    pub async fn claims(&self, token: &RequestToken, conn: &mut Conn<'_>) -> Result<Claims, Error> {
        match token {
            RequestToken::Bearer(token) => self.cipher.jwt.decode(token).map_err(|e| match e {
                token::jwt::Error::TokenExpired => {
                    let claims = self.cipher.jwt.decode_expired(token).ok();
                    Error::ExpiredJwt(
                        claims
                            .map_or_else(|| "unknown".to_string(), |c| format!("{}", c.resource())),
                    )
                }
                other => Error::DecodeJwt(other),
            }),
            RequestToken::ApiKey(token) => Validated::from_token(token, conn)
                .await
                .map_err(Error::ValidateApiKey)
                .map(|v| v.claims(self.token_expires)),
        }
    }

    pub async fn authorize_claims(
        &self,
        claims: Claims,
        perms: Perms,
        resources: Option<Resources>,
        conn: &mut Conn<'_>,
    ) -> Result<AuthZ, Error> {
        // first ensure that claims can access the requested resource
        let granted = if let Some(resources) = resources {
            claims.ensure_resources(resources, conn).await?
        } else {
            None
        };

        // then grant permissions from the access claims
        let granted = Granted::from_access(&claims.access, granted, conn).await?;

        // then grant permissions for non-org roles
        let granted = if let Some(user_id) = claims.resource().user() {
            Granted::all_orgs(user_id, Some(granted), conn).await?
        } else {
            granted
        };

        // finally check that the requested permissions exist
        match perms {
            Perms::One(perm) => granted.ensure_perm(perm, &claims)?,
            Perms::All(perms) => granted.ensure_all_perms(perms, &claims)?,
            Perms::Any(perms) => granted.ensure_any_perms(perms, &claims)?,
        }

        Ok(AuthZ { claims, granted })
    }

    pub fn refresh(&self, meta: &Metadata) -> Result<Refresh, Error> {
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
    pub fn maybe_refresh(&self, meta: &Metadata) -> Result<Option<Refresh>, Error> {
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

    /// Predicate to check if all of the permissions are granted.
    pub fn has_all_perms<I, P>(&self, perms: I) -> bool
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm>,
    {
        self.granted.has_all_perms(perms)
    }

    /// Predicate to check if any one of the permissions are granted.
    pub fn has_any_perms<I, P>(&self, perms: I) -> bool
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm>,
    {
        self.granted.has_any_perms(perms)
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
