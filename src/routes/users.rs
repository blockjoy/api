//! Routes namespaced by ***/users***

use crate::routes::*;

pub fn routes() -> Router {
    Router::new()
        .route("/", post(create_user))
        .route("/summary", get(users_summary))
        .route("/:id/orgs", get(list_user_orgs))
        .route("/:id/summary", get(user_summary))
        .route("/:id/payments", get(user_payments))
        .route("/:id/rewards/summary", get(get_reward_summary))
        .route("/:id/validators", get(list_validators_by_user))
        .route("/:id/invoices", get(list_invoices))
        .route("/:id/validators", post(stake_validator))
        .route("/:id/validators/staking/export", get(users_staking_export))
}
