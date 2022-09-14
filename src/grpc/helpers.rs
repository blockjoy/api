use crate::grpc::blockjoy_ui::{response_meta, Pagination, ResponseMeta, Uuid};
use prost_types::Timestamp;
use std::env;
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

pub fn success_response_meta(request_id: Option<Uuid>) -> ResponseMeta {
    ResponseMeta {
        status: response_meta::Status::Success.into(),
        origin_request_id: request_id,
        messages: vec![],
        pagination: None,
    }
}

pub fn success_response_with_pagination(request_id: Option<Uuid>) -> ResponseMeta {
    let max_items = env::var("PAGINATION_MAX_ITEMS")
        .unwrap()
        .parse::<i32>()
        .unwrap_or(10);
    let pagination = Pagination {
        total_items: Some(0i32),
        items_per_page: max_items,
        current_page: 0,
    };

    ResponseMeta {
        status: response_meta::Status::Success.into(),
        origin_request_id: request_id,
        messages: vec![],
        pagination: Some(pagination),
    }
}

pub fn pagination_parameters(pagination: Option<Pagination>) -> Result<(i32, i32), Status> {
    if let Some(..) = pagination {
        let pagination = pagination.unwrap();
        let max_items = env::var("PAGINATION_MAX_ITEMS")
            .unwrap()
            .parse::<i32>()
            .unwrap();

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
