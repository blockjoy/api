//! Routes namespaced by ***/commands***

use crate::http::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/:id", get(get_command))
        .route("/:id", delete(delete_command))
        // TODO: PUT /:id/response doesn't seem to be very RESTful, PUT /:id should be sufficient
        .route("/:id/response", put(update_command_response))
}
