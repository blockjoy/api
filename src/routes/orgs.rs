//! Routes namespaced by ***/orgs***

use crate::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", post(create_org))
        .route("/:id", get(get_org))
        .route("/:id", delete(delete_org))
        .route("/:id", put(update_org))
        .route("/:id/members", get(get_org_members))
        .route("/:id/broadcast_filters", get(list_org_broadcast_filters))
}
