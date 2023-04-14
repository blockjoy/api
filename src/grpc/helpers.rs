use crate::auth::JwtToken;
use crate::Error;
use prost_types::Timestamp;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::Status;

pub fn pb_current_timestamp() -> Timestamp {
    let start = SystemTime::now();
    let seconds = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64;
    let nanos = (start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos()
        * 1000) as i32;

    Timestamp { seconds, nanos }
}

pub fn required(name: &'static str) -> impl Fn() -> Status {
    move || Status::invalid_argument(format!("`{name}` is required"))
}

pub fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

pub fn try_get_token<T, R: JwtToken + Sync + Send + 'static>(
    req: &tonic::Request<T>,
) -> Result<&R, Error> {
    let tkn = req
        .extensions()
        .get::<R>()
        .ok_or_else(|| Status::internal("Token lost!"))?;

    Ok(tkn)
}
