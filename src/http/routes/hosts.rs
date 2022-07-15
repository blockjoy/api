//! Routes namespaced by ***/hosts***

use crate::http::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", get(list_hosts))
        .route("/", post(create_host))
        .route("/:id", get(get_host))
        .route("/:id", put(update_host))
        .route("/:id", delete(delete_host))
        .route("/:id/status", put(update_host_status))
        .route("/:id/commands", post(create_command))
        .route("/:id/commands", get(list_commands))
        .route("/:id/commands/pending", get(list_pending_commands))
        .route("/token/:token", get(get_host_by_token))
}
