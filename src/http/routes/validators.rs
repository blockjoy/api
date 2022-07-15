//! Routes namespaced by ***/validators***

use crate::http::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", get(list_validators))
        .route("/:id", get(get_validator))
        .route("/:id/migrate", post(migrate_validator))
        .route("/:id/status", put(update_validator_status))
        .route("/:id/stake_status", put(update_validator_stake_status))
        .route("/:id/owner_address", put(update_validator_owner_address))
        .route("/:id/penalty", put(update_validator_penalty))
        .route("/:id/identity", put(update_validator_identity))
        .route("/staking", get(list_validators_staking))
        .route("/consensus", get(list_validators_consensus))
        .route("/needs_attention", get(list_validators_attention))
        .route("/inventory/count", get(validator_inventory_count))
}
