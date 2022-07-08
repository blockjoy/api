//! Routes namespaced by ***/nodes***

use crate::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", post(create_node))
        .route("/:id", get(get_node))
        // TODO: PUT /:id/info doesn't seem to be very RESTful, PUT /:id should be sufficient
        .route("/:id/info", put(update_node_info))
}
