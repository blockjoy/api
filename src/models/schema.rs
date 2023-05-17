// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_blockchain_status"))]
    pub struct EnumBlockchainStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_conn_status"))]
    pub struct EnumConnStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_container_status"))]
    pub struct EnumContainerStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_host_cmd"))]
    pub struct EnumHostCmd;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_host_type"))]
    pub struct EnumHostType;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_chain_status"))]
    pub struct EnumNodeChainStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_log_event"))]
    pub struct EnumNodeLogEvent;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_resource_affinity"))]
    pub struct EnumNodeResourceAffinity;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_similarity_affinity"))]
    pub struct EnumNodeSimilarityAffinity;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_staking_status"))]
    pub struct EnumNodeStakingStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_sync_status"))]
    pub struct EnumNodeSyncStatus;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_type"))]
    pub struct EnumNodeType;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_org_role"))]
    pub struct EnumOrgRole;

    #[derive(diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "token_type"))]
    pub struct TokenType;
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumBlockchainStatus;

    blockchains (id) {
        id -> Uuid,
        name -> Text,
        description -> Nullable<Text>,
        status -> EnumBlockchainStatus,
        project_url -> Nullable<Text>,
        repo_url -> Nullable<Text>,
        version -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        supported_node_types -> Jsonb,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumHostCmd;

    commands (id) {
        id -> Uuid,
        host_id -> Uuid,
        cmd -> EnumHostCmd,
        sub_cmd -> Nullable<Text>,
        response -> Nullable<Text>,
        exit_status -> Nullable<Int4>,
        created_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
        node_id -> Nullable<Uuid>,
    }
}

diesel::table! {
    host_provisions (id) {
        id -> Text,
        created_at -> Timestamptz,
        claimed_at -> Nullable<Timestamptz>,
        host_id -> Nullable<Uuid>,
        ip_range_from -> Nullable<Inet>,
        ip_range_to -> Nullable<Inet>,
        ip_gateway -> Nullable<Inet>,
        org_id -> Nullable<Uuid>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumConnStatus;
    use super::sql_types::EnumHostType;

    hosts (id) {
        id -> Uuid,
        version -> Text,
        name -> Text,
        ip_addr -> Text,
        status -> EnumConnStatus,
        created_at -> Timestamptz,
        cpu_count -> Int8,
        mem_size_bytes -> Int8,
        disk_size_bytes -> Int8,
        os -> Text,
        os_version -> Text,
        ip_range_from -> Inet,
        ip_range_to -> Inet,
        ip_gateway -> Inet,
        used_cpu -> Nullable<Int4>,
        used_memory -> Nullable<Int8>,
        used_disk_space -> Nullable<Int8>,
        load_one -> Nullable<Float8>,
        load_five -> Nullable<Float8>,
        load_fifteen -> Nullable<Float8>,
        network_received -> Nullable<Int8>,
        network_sent -> Nullable<Int8>,
        uptime -> Nullable<Int8>,
        host_type -> Nullable<EnumHostType>,
        org_id -> Nullable<Uuid>,
    }
}

diesel::table! {
    invitations (id) {
        id -> Uuid,
        created_by_user -> Uuid,
        created_for_org -> Uuid,
        invitee_email -> Text,
        created_at -> Timestamptz,
        accepted_at -> Nullable<Timestamptz>,
        declined_at -> Nullable<Timestamptz>,
        created_by_user_name -> Text,
        created_for_org_name -> Text,
    }
}

diesel::table! {
    ip_addresses (id) {
        id -> Uuid,
        ip -> Inet,
        host_id -> Nullable<Uuid>,
        is_assigned -> Bool,
    }
}

diesel::table! {
    node_key_files (id) {
        id -> Uuid,
        name -> Text,
        content -> Text,
        node_id -> Uuid,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeLogEvent;
    use super::sql_types::EnumNodeType;

    node_logs (id) {
        id -> Uuid,
        host_id -> Uuid,
        node_id -> Uuid,
        event -> EnumNodeLogEvent,
        blockchain_name -> Text,
        node_type -> EnumNodeType,
        version -> Varchar,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeSyncStatus;
    use super::sql_types::EnumNodeChainStatus;
    use super::sql_types::EnumNodeStakingStatus;
    use super::sql_types::EnumContainerStatus;
    use super::sql_types::EnumNodeType;
    use super::sql_types::EnumNodeSimilarityAffinity;
    use super::sql_types::EnumNodeResourceAffinity;

    nodes (id) {
        id -> Uuid,
        org_id -> Uuid,
        host_id -> Uuid,
        name -> Text,
        version -> Text,
        ip_addr -> Text,
        address -> Nullable<Text>,
        wallet_address -> Nullable<Text>,
        block_height -> Nullable<Int8>,
        node_data -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        blockchain_id -> Uuid,
        sync_status -> EnumNodeSyncStatus,
        chain_status -> EnumNodeChainStatus,
        staking_status -> Nullable<EnumNodeStakingStatus>,
        container_status -> EnumContainerStatus,
        properties -> Jsonb,
        ip_gateway -> Text,
        self_update -> Bool,
        block_age -> Nullable<Int8>,
        consensus -> Nullable<Bool>,
        vcpu_count -> Int8,
        mem_size_bytes -> Int8,
        disk_size_bytes -> Int8,
        host_name -> Text,
        network -> Text,
        created_by -> Nullable<Uuid>,
        dns_record_id -> Varchar,
        allow_ips -> Jsonb,
        deny_ips -> Jsonb,
        node_type -> EnumNodeType,
        scheduler_similarity -> Nullable<EnumNodeSimilarityAffinity>,
        scheduler_resource -> Nullable<EnumNodeResourceAffinity>,
    }
}

diesel::table! {
    orgs (id) {
        id -> Uuid,
        name -> Text,
        is_personal -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        deleted_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumOrgRole;

    orgs_users (org_id, user_id) {
        org_id -> Uuid,
        user_id -> Uuid,
        role -> EnumOrgRole,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::TokenType;

    token_blacklist (token) {
        token -> Text,
        token_type -> TokenType,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        email -> Text,
        hashword -> Text,
        salt -> Text,
        created_at -> Timestamptz,
        first_name -> Varchar,
        last_name -> Varchar,
        confirmed_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(commands -> hosts (host_id));
diesel::joinable!(commands -> nodes (node_id));
diesel::joinable!(host_provisions -> hosts (host_id));
diesel::joinable!(host_provisions -> orgs (org_id));
diesel::joinable!(hosts -> orgs (org_id));
diesel::joinable!(invitations -> orgs (created_for_org));
diesel::joinable!(invitations -> users (created_by_user));
diesel::joinable!(ip_addresses -> hosts (host_id));
diesel::joinable!(node_key_files -> nodes (node_id));
diesel::joinable!(nodes -> blockchains (blockchain_id));
diesel::joinable!(nodes -> hosts (host_id));
diesel::joinable!(nodes -> orgs (org_id));
diesel::joinable!(nodes -> users (created_by));
diesel::joinable!(orgs_users -> orgs (org_id));
diesel::joinable!(orgs_users -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    blockchains,
    commands,
    host_provisions,
    hosts,
    invitations,
    ip_addresses,
    node_key_files,
    node_logs,
    nodes,
    orgs,
    orgs_users,
    token_blacklist,
    users,
);
