use anyhow::anyhow;
use axum::http::Request;
use hyper::Body;
use crate::errors::ApiError;
use crate::models::{DbPool, Host, User};

pub type AuthenticationResult<T> = Result<T, ApiError>;

pub async fn user(_request: &Request<Body>) -> AuthenticationResult<User> {
    // let db = request.extensions().get::<DbPool>()?;
    unimplemented!()
}

/// Add Host as request extension
/// TODO: For some reason, the DB can't be accessed via request.extensions()
pub async fn host(request: &Request<Body>, db: DbPool) -> AuthenticationResult<Host> {
    if let Some(token) = request
        .headers()
        .get("Authorization")
        .and_then(|hv| hv.to_str().ok())
        .and_then(|hv| {
            let words = hv.split("Bearer").collect::<Vec<&str>>();

            words.get(1).map(|w| w.trim())
        })
    {
        // let db: &DbPool = request.extensions().get().unwrap();

        Ok(Host::find_by_token(token, &db).await?)
    } else {
        Err(ApiError::InvalidAuthentication(anyhow!("Invalid auth token")))
    }
}