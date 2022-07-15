//! Routes namespaced by ***/broadcast_filters***

use crate::http::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", post(create_broadcast_filter))
        .route("/:id", get(get_broadcast_filter))
        .route("/:id", put(update_broadcast_filter))
        .route("/:id", delete(delete_broadcast_filter))
}
