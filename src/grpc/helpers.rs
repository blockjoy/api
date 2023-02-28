use crate::auth::JwtToken;
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::{response_meta, Pagination, ResponseMeta};
use prost_types::Timestamp;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::Status;

use super::blockjoy_ui::{ApiToken, RequestMeta};

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
) -> Result<&R, ApiError> {
    let tkn = req
        .extensions()
        .get::<R>()
        .ok_or_else(|| Status::internal("Token lost!"))?;

    Ok(tkn)
}

impl ResponseMeta {
    /// Creates a new `ResponseMeta` with the provided request id and the status `Success`.
    pub fn new(request_id: String, token: Option<ApiToken>) -> Self {
        Self {
            status: response_meta::Status::Success.into(),
            origin_request_id: request_id,
            messages: vec![],
            pagination: None,
            token,
        }
    }

    /// Extracts the request id from the provided `RequestMeta` and then creates a `Success`
    /// response with extracted request id, if there was one. Additionally adds the user auth
    /// token, because it may have been renewed
    pub fn from_meta(meta: impl Into<Option<RequestMeta>>, token: Option<ApiToken>) -> Self {
        let meta = meta.into();

        Self::new(
            meta.map(|m| m.id.unwrap_or_default())
                .unwrap_or_else(|| String::from("")),
            token,
        )
    }

    /// Sets the status of self to the provided value.
    #[must_use]
    pub fn with_status(self, status: response_meta::Status) -> Self {
        let status = status.into();
        Self { status, ..self }
    }

    /// Updates the messages list to a list with a single element, namely the Display impl of the
    /// provided value.
    #[must_use]
    pub fn with_message(self, message: impl std::fmt::Display) -> Self {
        Self {
            messages: vec![message.to_string()],
            ..self
        }
    }

    /// Sets the pagination of self to zero, and the max items to the correct value extracted from
    /// the environment config parameter.
    #[must_use]
    pub fn with_pagination(self) -> Self {
        let max_items: i32 = env::var("PAGINATION_MAX_ITEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let pagination = Pagination {
            total_items: Some(0i32),
            items_per_page: max_items,
            current_page: 0,
        };

        ResponseMeta {
            pagination: Some(pagination),
            ..self
        }
    }
}

impl RequestMeta {
    pub fn with_token(self, token: String) -> Self {
        Self {
            token: Some(ApiToken { value: token }),
            ..self
        }
    }

    pub fn with_pagination(self, pagination: Pagination) -> Self {
        Self {
            pagination: Some(pagination),
            ..self
        }
    }
}

pub fn pagination_parameters(pagination: Option<Pagination>) -> Result<(i64, i64), Status> {
    if let Some(pagination) = pagination {
        let items_per_page = pagination.items_per_page.into();
        let current_page: i64 = pagination.current_page.into();
        let max_items = env::var("PAGINATION_MAX_ITEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        if items_per_page > max_items {
            return Err(Status::cancelled("Max items exceeded"));
        }

        Ok((items_per_page, current_page * items_per_page))
    } else {
        Ok((10, 0))
    }
}
