use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::{response_meta, Pagination, ResponseMeta};
use crate::models::{self, Node, NodeTypeKey};
use heck::ToLowerCamelCase;
use prost_types::Timestamp;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::Status;

use super::blockjoy_ui::RequestMeta;

pub fn image_url_from_node(node: &Node, chain_name: String) -> String {
    let node_type = NodeTypeKey::str_from_value(node.node_type.0.get_id()).to_lowercase();
    let version = node
        .version
        .clone()
        .unwrap_or_else(|| "latest".to_string())
        .to_lowercase();

    format!(
        "{}/{}/{}",
        chain_name.to_lower_camel_case(),
        node_type,
        version
    )
}

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

pub fn try_get_token<T>(req: &tonic::Request<T>) -> Result<models::Token, ApiError> {
    let tkn = req
        .extensions()
        .get::<models::Token>()
        .ok_or_else(|| Status::internal("Token lost!"))?
        .clone();
    Ok(tkn)
}

impl ResponseMeta {
    /// Creates a new `ResponseMeta` with the provided request id and the status `Success`.
    pub fn new(request_id: String) -> Self {
        Self {
            status: response_meta::Status::Success.into(),
            origin_request_id: request_id,
            messages: vec![],
            pagination: None,
        }
    }

    /// Extracts the request id from the provided `RequestMeta` and then creates a `Success`
    /// response with extracted request id, if there was one.
    pub fn from_meta(meta: impl Into<Option<RequestMeta>>) -> Self {
        let meta = meta.into();
        Self::new(meta.map(|m| m.id).unwrap_or_else(|| String::from("")))
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

pub fn pagination_parameters(pagination: Option<Pagination>) -> Result<(i32, i32), Status> {
    if let Some(pagination) = pagination {
        let max_items: i32 = env::var("PAGINATION_MAX_ITEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        if pagination.items_per_page > max_items {
            return Err(Status::cancelled("Max items exceeded"));
        }

        Ok((
            pagination.items_per_page,
            pagination.current_page * pagination.items_per_page,
        ))
    } else {
        Ok((10, 0))
    }
}
