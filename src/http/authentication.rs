use anyhow::anyhow;
use axum::http;
use axum::http::Request;
use crate::errors::ApiError;
use crate::auth::FindableByToken;
use crate::models::DbPool;

pub type AuthenticationResult<T> = Result<T, ApiError>;

/// Return authenticated resource (currently Host/User)
pub async fn resource<B, R>(
    request: &Request<B>,
) -> AuthenticationResult<R>
    where R: FindableByToken
{
    if let Some(token) = get_auth_token(request) {
        let db = request.extensions().get::<DbPool>().unwrap();

        Ok(R::find_by_token(token, &db.clone()).await?)
    } else {
        Err(ApiError::InvalidAuthentication(anyhow!("Invalid auth token")))
    }
}

/// Extract auth token from headers
fn get_auth_token<B>(request: &Request<B>) -> Option<&str> {
    request
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|hv| {
            let words = hv.split("Bearer").collect::<Vec<&str>>();

            words.get(1).map(|w| w.trim())
        })
}