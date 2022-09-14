//! # Authorization layer
//!
//!

use crate::auth::unauthenticated_paths::UnauthenticatedPaths;
use crate::auth::JwtToken;
use crate::auth::{Authorization, AuthorizationData, AuthorizationState};
use crate::models::Token;
use crate::server::DbPool;
use futures_util::future::BoxFuture;
use hyper::{Request, Response};
use std::fmt::Debug;
use tonic::body::BoxBody;
use tonic::Status;
use tower_http::auth::AsyncAuthorizeRequest;

use super::AnyToken;

fn unauthorized_response(msg: &str) -> Response<BoxBody> {
    Status::permission_denied(msg).to_http()
}

fn unauthenticated_response(msg: &str) -> Response<BoxBody> {
    Status::unauthenticated(msg).to_http()
}

#[derive(Clone)]
pub struct AuthorizationService {
    enforcer: Authorization,
}

impl AuthorizationService {
    pub fn new(enforcer: Authorization) -> Self {
        Self { enforcer }
    }

    pub fn is_unauthenticated_request<B>(&self, request: &Request<B>) -> bool {
        let unauth_paths = request.extensions().get::<UnauthenticatedPaths>().unwrap();

        unauth_paths.is_unauthenticated(request.uri().path())
    }
}

impl<B> AsyncAuthorizeRequest<B> for AuthorizationService
where
    B: Send + Sync + Debug + 'static,
{
    type RequestBody = B;
    type ResponseBody = BoxBody;
    type Future = BoxFuture<'static, Result<Request<B>, Response<Self::ResponseBody>>>;

    fn authorize(&mut self, mut request: Request<B>) -> Self::Future {
        if self.is_unauthenticated_request(&request) {
            tracing::debug!("Request is unauthenticated: {}", request.uri().path());
            return Box::pin(async move { Ok(request) });
        }

        let enforcer = self.enforcer.clone();

        Box::pin(async move {
            let token = AnyToken::from_request(&request)
                .map_err(|_| unauthenticated_response("Missing valid token"))?;
            let db = request
                .extensions()
                .get::<DbPool>()
                .unwrap_or_else(|| panic!("DB extension missing"));
            let cant_parse =
                |e| unauthenticated_response(&format!("Could not extract token: {e:?}"));
            let encoded = match token {
                AnyToken::Auth(auth) => auth.encode().map_err(cant_parse)?,
                AnyToken::PwdReset(pwd_reset) => pwd_reset.encode().map_err(cant_parse)?,
            };
            // TODO: maybe not unwrap here?
            let db_token = Token::find_by_token(&encoded, db).await.unwrap();
            let auth_data = AuthorizationData {
                subject: db_token.role.to_string(),
                object: request.uri().path().to_string(),
                action: request.method().to_string(),
            };

            let result = enforcer
                .try_authorized(auth_data)
                .map_err(|e| unauthorized_response(&e.to_string()))?;
            // Evaluate authorization result
            match result {
                AuthorizationState::Authorized => {
                    request.extensions_mut().insert(db_token);

                    Ok(request)
                }
                AuthorizationState::Denied => Err(unauthorized_response("Insufficient privileges")),
            }
        })
    }
}

#[cfg(test)]
mod tests {}
