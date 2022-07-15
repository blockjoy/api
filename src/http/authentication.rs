use axum::body::BoxBody;
use axum::http::Request;
use crate::errors::ApiError;
use crate::models::{Host, User};

pub type AuthenticationResult<T> = Result<T, ApiError>;

pub fn user(mut _request: &Request<BoxBody>) -> AuthenticationResult<User> {
    // let db = request.extensions().get::<DbPool>()?;
    unimplemented!()
}

pub fn host(mut _request: &Request<BoxBody>) -> AuthenticationResult<Host> {
    unimplemented!()
}