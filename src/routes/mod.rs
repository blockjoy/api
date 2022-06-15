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
        .route("/users", post(create_user))
        .route("/users/summary", get(users_summary))
        .route("/users/:id/orgs", get(list_user_orgs))
        .route("/users/:id/summary", get(user_summary))
        .route("/users/:id/payments", get(user_payments))
        .route("/hosts", get(list_hosts))
        .route("/hosts/token/:token", get(get_host_by_token))
        .route("/hosts/:id", get(get_host))
        .route("/hosts", post(create_host))
        .route("/hosts/:id", put(update_host))
        .route("/hosts/:id/status", put(update_host_status))
        .route("/hosts/:id", delete(delete_host))
        .route("/host_provisions", post(create_host_provision))
        .route("/host_provisions/:id/hosts", post(claim_host_provision))
        .route("/host_provisions/:id", get(get_host_provision))
        .route("/validators/:id/migrate", post(migrate_validator))
        .route("/validators", get(list_validators))
        .route("/validators/staking", get(list_validators_staking))
        .route("/validators/consensus", get(list_validators_consensus))
        .route(
            "/validators/needs_attention",
            get(list_validators_attention),
        )
        .route(
            "/validators/inventory/count",
            get(validator_inventory_count),
        )
        .route(
            "/users/:id/validators/staking/export",
            get(users_staking_export),
        )
        .route("/users/:id/validators", get(list_validators_by_user))
        .route("/users/:id/invoices", get(list_invoices))
        .route("/payments_due", get(list_payments_due))
        .route("/pay_adresses", get(list_pay_addresses))
        .route("/users/:id/validators", post(stake_validator))
        .route("/validators/:id", get(get_validator))
        .route("/validators/:id/status", put(update_validator_status))
        .route(
            "/validators/:id/stake_status",
            put(update_validator_stake_status),
        )
        .route(
            "/validators/:id/owner_address",
            put(update_validator_owner_address),
        )
        .route("/validators/:id/penalty", put(update_validator_penalty))
        .route("/validators/:id/identity", put(update_validator_identity))
        .route("/users/:id/rewards/summary", get(get_reward_summary))
        .route("/rewards", post(create_rewards))
        .route("/payments", post(create_payments))
        .route("/commands/:id", get(get_command))
        .route("/hosts/:id/commands", get(list_commands))
        .route("/hosts/:id/commands/pending", get(list_pending_commands))
        .route("/hosts/:id/commands", post(create_command))
        .route("/commands/:id/response", put(update_command_response))
        .route("/command/:id", delete(delete_command))
        .route("/qr/:id", get(get_qr))
        .route("/groups/nodes", get(list_node_groups))
        .route("/groups/nodes/:id", get(get_node_group))
        .route("/nodes/:id", get(get_node))
        .route("/nodes", post(create_node))
        .route("/nodes/:id/info", put(update_node_info))
        .route("/blockchains", get(list_blockchains))
        .route("/broadcast_filters", post(create_broadcast_filter))
        .route("/broadcast_filters/:id", get(get_broadcast_filter))
        .route(
            "/orgs/:id/broadcast_filters",
            get(list_org_broadcast_filters),
        )
        .route("/broadcast_filters/:id", put(update_broadcast_filter))
        .route("/broadcast_filters/:id", delete(delete_broadcast_filter))
        .route("/orgs", post(create_org))
        .route("/orgs/:id", get(get_org))
        .route("/orgs/:id", delete(delete_org))
        .route("/orgs/:id", put(update_org))
        .route("/orgs/:id/members", get(get_org_members))
        .route("/groups", post(create_group))
}
