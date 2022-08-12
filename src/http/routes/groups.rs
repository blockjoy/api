//! Routes namespaced by ***/groups***

use crate::http::handlers::*;
use crate::http::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/nodes", get(list_node_groups))
        .route("/nodes/:id", get(get_node_group))
}
