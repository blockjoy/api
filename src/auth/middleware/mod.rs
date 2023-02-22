//! # Authorization layer
//!
//!

use crate::auth::unauthenticated_paths::UnauthenticatedPaths;
use crate::auth::{Authorization, AuthorizationData, AuthorizationState, FindableById, TokenType};
use crate::auth::{JwtToken, UserRefreshToken};
use crate::errors::Result;
use crate::models;
use crate::models::{Host, User};
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

    pub fn is_unauthenticated_request<B: Debug>(&self, request: &Request<B>) -> bool {
        if let Some(unauth_paths) = request.extensions().get::<UnauthenticatedPaths>() {
            unauth_paths.is_unauthenticated(request.uri().path())
        } else {
            tracing::error!(
                "Request {request:?} did not contain `UnauthenicatedPaths` extension! \
                Blockvisor-api is misconfigured!"
            );
            false
        }
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
            let db = request
                .extensions()
                .get::<models::DbPool>()
                .unwrap_or_else(|| panic!("DB extension missing"));
            let token = AnyToken::from_request(&request)
                .map_err(|_| unauthenticated_response("Missing valid token"))?;
            let cant_parse =
                |e| unauthenticated_response(&format!("Could not extract token: {e:?}"));

            match token {
                AnyToken::PwdReset(token) => {
                    // 1. try if token is valid
                    token.encode().map_err(cant_parse)?;

                    let auth_data = AuthorizationData {
                        subject: token.role.to_string(),
                        object: request.uri().path().to_string(),
                        action: request.method().to_string(),
                    };

                    let result = enforcer
                        .try_authorized(auth_data)
                        .map_err(|e| unauthorized_response(&e.to_string()))?;
                    // Evaluate authorization result
                    match result {
                        AuthorizationState::Authorized => {
                            request.extensions_mut().insert(token);

                            Ok(request)
                        }
                        AuthorizationState::Denied => {
                            Err(unauthorized_response("Insufficient privileges"))
                        }
                    }
                }
                AnyToken::RegistrationConfirmation(token) => {
                    token.encode().map_err(cant_parse)?;

                    let auth_data = AuthorizationData {
                        subject: token.role.to_string(),
                        object: request.uri().path().to_string(),
                        action: request.method().to_string(),
                    };
                    let result = enforcer
                        .try_authorized(auth_data)
                        .map_err(|e| unauthorized_response(&e.to_string()))?;
                    // Evaluate authorization result
                    match result {
                        AuthorizationState::Authorized => {
                            request.extensions_mut().insert(token);

                            Ok(request)
                        }
                        AuthorizationState::Denied => {
                            Err(unauthorized_response("Insufficient privileges"))
                        }
                    }
                }
                AnyToken::Invitation(token) => {
                    token.encode().map_err(cant_parse)?;

                    let auth_data = AuthorizationData {
                        subject: token.role.to_string(),
                        object: request.uri().path().to_string(),
                        action: request.method().to_string(),
                    };
                    let result = enforcer
                        .try_authorized(auth_data)
                        .map_err(|e| unauthorized_response(&e.to_string()))?;
                    // Evaluate authorization result
                    match result {
                        AuthorizationState::Authorized => {
                            request.extensions_mut().insert(token);

                            Ok(request)
                        }
                        AuthorizationState::Denied => {
                            Err(unauthorized_response("Insufficient privileges"))
                        }
                    }
                }
                AnyToken::UserAuth(token) => {
                    // 1. try if token is valid
                    token.encode().map_err(cant_parse)?;

                    let mut tx = db
                        .begin()
                        .await
                        .map_err(|_| Status::unauthenticated("Db down :(").to_http())?;
                    let refresh_token = match UserRefreshToken::from_request(&request) {
                        Ok(token) => token,
                        Err(e) => {
                            tracing::error!("No refresh token in request: {e}");
                            let refresh = User::find_by_id(token.get_id(), &mut tx)
                                .await
                                .map_err(|_| unauthenticated_response("No refresh token for user"))?
                                .refresh
                                .unwrap_or_default();

                            UserRefreshToken::from_encoded(
                                refresh.as_str(),
                                TokenType::UserRefresh,
                                true,
                            )
                            .map_err(|e| {
                                unauthenticated_response(&format!(
                                    "Could not extract refresh token: {e:?}"
                                ))
                            })?
                        }
                    };
                    let (_, token, refresh_token) =
                        User::verify_and_refresh_auth_token(token, refresh_token, &mut tx)
                            .await
                            .map_err(|e| Status::from(e).to_http())?;
                    tx.commit().await.map_err(|_| {
                        Status::unauthenticated("Could not commit to db :(").to_http()
                    })?;
                    let auth_data = AuthorizationData {
                        subject: token.role.to_string(),
                        object: request.uri().path().to_string(),
                        action: request.method().to_string(),
                    };

                    let result = enforcer
                        .try_authorized(auth_data)
                        .map_err(|e| unauthorized_response(&e.to_string()))?;
                    // Evaluate authorization result
                    match result {
                        AuthorizationState::Authorized => {
                            request.extensions_mut().insert(token);
                            request.extensions_mut().insert(refresh_token);

                            Ok(request)
                        }
                        AuthorizationState::Denied => {
                            Err(unauthorized_response("Insufficient privileges"))
                        }
                    }
                }
                AnyToken::HostAuth(token) => {
                    // 1. try if token is valid
                    token.encode().map_err(cant_parse)?;

                    let token =
                        Host::verify_auth_token(token).map_err(|e| Status::from(e).to_http())?;

                    let auth_data = AuthorizationData {
                        subject: token.role.to_string(),
                        object: request.uri().path().to_string(),
                        action: request.method().to_string(),
                    };

                    let result = enforcer
                        .try_authorized(auth_data)
                        .map_err(|e| unauthorized_response(&e.to_string()))?;
                    // Evaluate authorization result
                    match result {
                        AuthorizationState::Authorized => {
                            request.extensions_mut().insert(token);

                            Ok(request)
                        }
                        AuthorizationState::Denied => {
                            Err(unauthorized_response("Insufficient privileges"))
                        }
                    }
                }
                _ => Err(unauthorized_response("Invalid token type")),
            }
        })
    }
}
