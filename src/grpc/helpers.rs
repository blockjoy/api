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

// pub fn pagination_parameters(pagination: Option<api::Pagination>) -> Result<(i64, i64), Status> {
//     if let Some(pagination) = pagination {
//         let items_per_page = pagination.items_per_page.into();
//         let current_page: i64 = pagination.current_page.into();
//         let max_items = env::var("PAGINATION_MAX_ITEMS")
//             .ok()
//             .and_then(|s| s.parse().ok())
//             .unwrap_or(10);

//         if items_per_page > max_items {
//             return Err(Status::cancelled("Max items exceeded"));
//         }

//         Ok((items_per_page, current_page * items_per_page))
//     } else {
//         Ok((10, 0))
//     }
// }
