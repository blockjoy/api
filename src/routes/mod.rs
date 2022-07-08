//! routes/mod.rs

mod node;
mod group;
mod command;
mod validators;
mod broadcast_filters;
mod orgs;
mod users;
mod hosts;
mod host_provisions;

use crate::handlers::*;
use axum::routing::{delete, get, post, put};
use axum::Router;

pub fn api_router() -> Router {
    Router::new()
        .route("/reset", post(reset_pwd))
        .route("/reset", put(update_pwd))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/whoami", get(whoami))
        .route("/block_height", get(get_block_height))
        .route("/block_info", get(get_block_info))
        .route("/block_info", put(update_block_info))
        .route("/payments_due", get(list_payments_due))
        .route("/pay_adresses", get(list_pay_addresses))
        .route("/rewards", post(create_rewards))
        .route("/payments", post(create_payments))
        .route("/qr/:id", get(get_qr))

        .route("/blockchains", get(list_blockchains))
        // Nested routes
        .nest("/orgs", orgs::routes())
        .nest("/users", users::routes())
        .nest("/hosts", hosts::routes())
        .nest("/host_provisions", host_provisions::routes())
        .nest("/broadcast_filters", broadcast_filters::routes())
        .nest("/validators", validators::routes())
        .nest("/commands", command::routes())
        .nest("/nodes", node::routes())
        .nest("/groups", group::routes())
}
