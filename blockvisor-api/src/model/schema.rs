// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "blockchain_property_ui_type"))]
    pub struct BlockchainPropertyUiType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_command_exit_code"))]
    pub struct EnumCommandExitCode;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_command_type"))]
    pub struct EnumCommandType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_config_type"))]
    pub struct EnumConfigType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_connection_status"))]
    pub struct EnumConnectionStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_container_status"))]
    pub struct EnumContainerStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_firewall_action"))]
    pub struct EnumFirewallAction;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_firewall_direction"))]
    pub struct EnumFirewallDirection;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_firewall_protocol"))]
    pub struct EnumFirewallProtocol;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_health"))]
    pub struct EnumHealth;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_host_type_old"))]
    pub struct EnumHostTypeOld;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_next_state"))]
    pub struct EnumNextState;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_event"))]
    pub struct EnumNodeEvent;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_log_event"))]
    pub struct EnumNodeLogEvent;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_resource_affinity"))]
    pub struct EnumNodeResourceAffinity;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_similarity_affinity"))]
    pub struct EnumNodeSimilarityAffinity;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_staking_status"))]
    pub struct EnumNodeStakingStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_state"))]
    pub struct EnumNodeState;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_status"))]
    pub struct EnumNodeStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_sync_status"))]
    pub struct EnumNodeSyncStatus;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_node_type"))]
    pub struct EnumNodeType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_resource_type"))]
    pub struct EnumResourceType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_schedule_type"))]
    pub struct EnumScheduleType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_token_type"))]
    pub struct EnumTokenType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_ui_type"))]
    pub struct EnumUiType;

    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "enum_visibility"))]
    pub struct EnumVisibility;
}

diesel::table! {
    addresses (id) {
        id -> Uuid,
        #[max_length = 256]
        city -> Nullable<Varchar>,
        #[max_length = 256]
        country -> Nullable<Varchar>,
        #[max_length = 256]
        line1 -> Nullable<Varchar>,
        #[max_length = 256]
        line2 -> Nullable<Varchar>,
        #[max_length = 256]
        postal_code -> Nullable<Varchar>,
        #[max_length = 256]
        state -> Nullable<Varchar>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumResourceType;

    api_keys (id) {
        id -> Uuid,
        user_id -> Uuid,
        label -> Text,
        key_hash -> Text,
        key_salt -> Text,
        resource -> EnumResourceType,
        resource_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    archives (id) {
        id -> Uuid,
        org_id -> Nullable<Uuid>,
        image_id -> Uuid,
        store_id -> Text,
        image_property_ids -> Array<Nullable<Uuid>>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeType;
    use super::sql_types::EnumVisibility;

    blockchain_node_types_old (id) {
        id -> Uuid,
        blockchain_id -> Uuid,
        description -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        node_type -> EnumNodeType,
        visibility -> EnumVisibility,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::BlockchainPropertyUiType;

    blockchain_properties_old (id) {
        id -> Uuid,
        blockchain_id -> Uuid,
        name -> Text,
        default -> Nullable<Text>,
        ui_type -> BlockchainPropertyUiType,
        disabled -> Bool,
        required -> Bool,
        blockchain_node_type_id -> Uuid,
        blockchain_version_id -> Uuid,
        display_name -> Text,
    }
}

diesel::table! {
    blockchain_versions_old (id) {
        id -> Uuid,
        blockchain_id -> Uuid,
        blockchain_node_type_id -> Uuid,
        version -> Text,
        description -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumVisibility;

    blockchains_old (id) {
        id -> Uuid,
        name -> Text,
        description -> Nullable<Text>,
        project_url -> Nullable<Text>,
        repo_url -> Nullable<Text>,
        version -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        visibility -> EnumVisibility,
        ticker -> Text,
        display_name -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumCommandExitCode;
    use super::sql_types::EnumCommandType;

    commands (id) {
        id -> Uuid,
        host_id -> Uuid,
        exit_message -> Nullable<Text>,
        created_at -> Timestamptz,
        completed_at -> Nullable<Timestamptz>,
        node_id -> Nullable<Uuid>,
        acked_at -> Nullable<Timestamptz>,
        retry_hint_seconds -> Nullable<Int8>,
        exit_code -> Nullable<EnumCommandExitCode>,
        command_type -> EnumCommandType,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumConfigType;
    use super::sql_types::EnumResourceType;

    configs (id) {
        id -> Uuid,
        image_id -> Uuid,
        archive_id -> Uuid,
        config_type -> EnumConfigType,
        config -> Bytea,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumScheduleType;
    use super::sql_types::EnumConnectionStatus;
    use super::sql_types::EnumResourceType;

    hosts (id) {
        id -> Uuid,
        org_id -> Nullable<Uuid>,
        region_id -> Nullable<Uuid>,
        network_name -> Text,
        display_name -> Nullable<Text>,
        schedule_type -> EnumScheduleType,
        connection_status -> EnumConnectionStatus,
        cpu_cores -> Int8,
        memory_bytes -> Int8,
        disk_bytes -> Int8,
        os -> Text,
        os_version -> Text,
        bv_version -> Text,
        ip_address -> Inet,
        ip_gateway -> Inet,
        node_count -> Int8,
        node_cpu_cores -> Int8,
        node_memory_bytes -> Int8,
        node_disk_bytes -> Int8,
        used_cpu_hundreths -> Nullable<Int8>,
        used_memory_bytes -> Nullable<Int8>,
        used_disk_bytes -> Nullable<Int8>,
        load_one_percent -> Nullable<Float8>,
        load_five_percent -> Nullable<Float8>,
        load_fifteen_percent -> Nullable<Float8>,
        network_received_bytes -> Nullable<Int8>,
        network_sent_bytes -> Nullable<Int8>,
        uptime_seconds -> Nullable<Int8>,
        tags -> Array<Nullable<Text>>,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumConnectionStatus;
    use super::sql_types::EnumHostTypeOld;
    use super::sql_types::EnumScheduleType;

    hosts_old (id) {
        id -> Uuid,
        version -> Text,
        name -> Text,
        ip_addr -> Text,
        status -> EnumConnectionStatus,
        created_at -> Timestamptz,
        cpu_count -> Int8,
        mem_size_bytes -> Int8,
        disk_size_bytes -> Int8,
        os -> Text,
        os_version -> Text,
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
        host_type -> Nullable<EnumHostTypeOld>,
        org_id -> Uuid,
        created_by -> Nullable<Uuid>,
        region_id -> Nullable<Uuid>,
        monthly_cost_in_usd -> Nullable<Int8>,
        vmm_mountpoint -> Nullable<Text>,
        deleted_at -> Nullable<Timestamptz>,
        managed_by -> EnumScheduleType,
        node_count -> Int4,
        tags -> Array<Nullable<Text>>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumUiType;

    image_properties (id) {
        id -> Uuid,
        image_id -> Uuid,
        key -> Text,
        key_group -> Nullable<Text>,
        is_group_default -> Nullable<Bool>,
        new_archive -> Bool,
        default_value -> Text,
        dynamic_value -> Bool,
        description -> Nullable<Text>,
        ui_type -> EnumUiType,
        add_cpu_cores -> Nullable<Int8>,
        add_memory_bytes -> Nullable<Int8>,
        add_disk_bytes -> Nullable<Int8>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumFirewallProtocol;
    use super::sql_types::EnumFirewallDirection;
    use super::sql_types::EnumFirewallAction;

    image_rules (id) {
        id -> Uuid,
        image_id -> Uuid,
        key -> Text,
        description -> Nullable<Text>,
        protocol -> EnumFirewallProtocol,
        direction -> EnumFirewallDirection,
        action -> EnumFirewallAction,
        ips -> Nullable<Jsonb>,
        ports -> Nullable<Jsonb>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumFirewallAction;
    use super::sql_types::EnumVisibility;

    images (id) {
        id -> Uuid,
        org_id -> Nullable<Uuid>,
        protocol_version_id -> Uuid,
        image_uri -> Text,
        build_version -> Int8,
        description -> Nullable<Text>,
        min_cpu_cores -> Int8,
        min_memory_bytes -> Int8,
        min_disk_bytes -> Int8,
        ramdisks -> Jsonb,
        default_firewall_in -> EnumFirewallAction,
        default_firewall_out -> EnumFirewallAction,
        visibility -> EnumVisibility,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumResourceType;

    invitations (id) {
        id -> Uuid,
        invited_by -> Uuid,
        org_id -> Uuid,
        invitee_email -> Text,
        created_at -> Timestamptz,
        accepted_at -> Nullable<Timestamptz>,
        declined_at -> Nullable<Timestamptz>,
        invited_by_resource -> EnumResourceType,
    }
}

diesel::table! {
    ip_addresses (id) {
        id -> Uuid,
        ip -> Inet,
        host_id -> Uuid,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeEvent;
    use super::sql_types::EnumResourceType;

    node_logs (id) {
        id -> Uuid,
        node_id -> Uuid,
        host_id -> Uuid,
        event -> EnumNodeEvent,
        event_data -> Nullable<Jsonb>,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeLogEvent;
    use super::sql_types::EnumNodeType;

    node_logs_old (id) {
        id -> Uuid,
        host_id -> Uuid,
        node_id -> Uuid,
        event -> EnumNodeLogEvent,
        #[max_length = 32]
        version -> Varchar,
        created_at -> Timestamptz,
        node_type -> EnumNodeType,
        org_id -> Uuid,
        blockchain_id -> Uuid,
    }
}

diesel::table! {
    node_properties_old (id) {
        id -> Uuid,
        node_id -> Uuid,
        blockchain_property_id -> Uuid,
        value -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumResourceType;

    node_reports (id) {
        id -> Uuid,
        node_id -> Uuid,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        message -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeState;
    use super::sql_types::EnumNextState;
    use super::sql_types::EnumHealth;
    use super::sql_types::EnumNodeSimilarityAffinity;
    use super::sql_types::EnumNodeResourceAffinity;
    use super::sql_types::EnumResourceType;

    nodes (id) {
        id -> Uuid,
        node_name -> Text,
        display_name -> Nullable<Text>,
        old_node_id -> Nullable<Uuid>,
        org_id -> Uuid,
        host_id -> Uuid,
        image_id -> Uuid,
        config_id -> Uuid,
        protocol_id -> Uuid,
        protocol_version_id -> Uuid,
        semantic_version -> Text,
        auto_upgrade -> Bool,
        node_state -> EnumNodeState,
        next_state -> Nullable<EnumNextState>,
        protocol_state -> Nullable<Text>,
        protocol_health -> Nullable<EnumHealth>,
        jobs -> Nullable<Jsonb>,
        note -> Nullable<Text>,
        tags -> Array<Nullable<Text>>,
        ip_address -> Inet,
        ip_gateway -> Inet,
        p2p_address -> Nullable<Text>,
        dns_id -> Text,
        dns_name -> Text,
        dns_url -> Nullable<Text>,
        cpu_cores -> Int8,
        memory_bytes -> Int8,
        disk_bytes -> Int8,
        block_height -> Nullable<Int8>,
        block_age -> Nullable<Int8>,
        consensus -> Nullable<Bool>,
        scheduler_similarity -> Nullable<EnumNodeSimilarityAffinity>,
        scheduler_resource -> Nullable<EnumNodeResourceAffinity>,
        scheduler_region_id -> Nullable<Uuid>,
        stripe_item_id -> Nullable<Text>,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumNodeSimilarityAffinity;
    use super::sql_types::EnumNodeResourceAffinity;
    use super::sql_types::EnumResourceType;
    use super::sql_types::EnumNodeType;
    use super::sql_types::EnumContainerStatus;
    use super::sql_types::EnumNodeSyncStatus;
    use super::sql_types::EnumNodeStakingStatus;
    use super::sql_types::EnumNodeStatus;

    nodes_old (id) {
        id -> Uuid,
        org_id -> Uuid,
        host_id -> Uuid,
        node_name -> Text,
        version -> Text,
        address -> Nullable<Text>,
        wallet_address -> Nullable<Text>,
        block_height -> Nullable<Int8>,
        node_data -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        blockchain_id -> Uuid,
        ip_gateway -> Text,
        self_update -> Bool,
        block_age -> Nullable<Int8>,
        consensus -> Nullable<Bool>,
        vcpu_count -> Int8,
        mem_size_bytes -> Int8,
        disk_size_bytes -> Int8,
        network -> Text,
        created_by -> Nullable<Uuid>,
        #[max_length = 50]
        dns_record_id -> Varchar,
        allow_ips -> Jsonb,
        deny_ips -> Jsonb,
        scheduler_similarity -> Nullable<EnumNodeSimilarityAffinity>,
        scheduler_resource -> Nullable<EnumNodeResourceAffinity>,
        scheduler_region -> Nullable<Uuid>,
        data_directory_mountpoint -> Nullable<Text>,
        jobs -> Jsonb,
        created_by_resource -> Nullable<EnumResourceType>,
        deleted_at -> Nullable<Timestamptz>,
        node_type -> EnumNodeType,
        container_status -> EnumContainerStatus,
        sync_status -> EnumNodeSyncStatus,
        staking_status -> Nullable<EnumNodeStakingStatus>,
        note -> Nullable<Text>,
        node_status -> EnumNodeStatus,
        url -> Text,
        ip -> Inet,
        dns_name -> Text,
        display_name -> Text,
        stripe_item_id -> Nullable<Text>,
        old_node_id -> Nullable<Uuid>,
        tags -> Array<Nullable<Text>>,
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
        host_count -> Int4,
        node_count -> Int4,
        member_count -> Int4,
        stripe_customer_id -> Nullable<Text>,
        address_id -> Nullable<Uuid>,
    }
}

diesel::table! {
    permissions (name) {
        name -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumVisibility;

    protocol_versions (id) {
        id -> Uuid,
        org_id -> Nullable<Uuid>,
        protocol_id -> Uuid,
        protocol_key -> Text,
        variant_key -> Text,
        semantic_version -> Text,
        sku_code -> Text,
        description -> Nullable<Text>,
        visibility -> EnumVisibility,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumVisibility;

    protocols (id) {
        id -> Uuid,
        org_id -> Nullable<Uuid>,
        key -> Text,
        name -> Text,
        description -> Nullable<Text>,
        ticker -> Nullable<Text>,
        visibility -> EnumVisibility,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    regions (id) {
        id -> Uuid,
        name -> Text,
        pricing_tier -> Nullable<Text>,
    }
}

diesel::table! {
    role_permissions (role, permission) {
        role -> Text,
        permission -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    roles (name) {
        name -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EnumTokenType;
    use super::sql_types::EnumResourceType;

    tokens (id) {
        id -> Uuid,
        token_type -> EnumTokenType,
        token -> Text,
        created_by_type -> EnumResourceType,
        created_by_id -> Uuid,
        org_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    user_roles (user_id, org_id, role) {
        user_id -> Uuid,
        org_id -> Uuid,
        role -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    user_settings (id) {
        id -> Uuid,
        user_id -> Uuid,
        key -> Text,
        value -> Bytea,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        email -> Text,
        hashword -> Text,
        salt -> Text,
        created_at -> Timestamptz,
        #[max_length = 64]
        first_name -> Varchar,
        #[max_length = 64]
        last_name -> Varchar,
        confirmed_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(api_keys -> users (user_id));
diesel::joinable!(archives -> images (image_id));
diesel::joinable!(archives -> orgs (org_id));
diesel::joinable!(blockchain_node_types_old -> blockchains_old (blockchain_id));
diesel::joinable!(blockchain_properties_old -> blockchain_node_types_old (blockchain_node_type_id));
diesel::joinable!(blockchain_properties_old -> blockchain_versions_old (blockchain_version_id));
diesel::joinable!(blockchain_properties_old -> blockchains_old (blockchain_id));
diesel::joinable!(blockchain_versions_old -> blockchain_node_types_old (blockchain_node_type_id));
diesel::joinable!(blockchain_versions_old -> blockchains_old (blockchain_id));
diesel::joinable!(commands -> hosts (host_id));
diesel::joinable!(commands -> nodes (node_id));
diesel::joinable!(configs -> archives (archive_id));
diesel::joinable!(configs -> images (image_id));
diesel::joinable!(hosts -> orgs (org_id));
diesel::joinable!(hosts -> regions (region_id));
diesel::joinable!(hosts_old -> orgs (org_id));
diesel::joinable!(hosts_old -> regions (region_id));
diesel::joinable!(hosts_old -> users (created_by));
diesel::joinable!(image_properties -> images (image_id));
diesel::joinable!(image_rules -> images (image_id));
diesel::joinable!(images -> orgs (org_id));
diesel::joinable!(images -> protocol_versions (protocol_version_id));
diesel::joinable!(invitations -> orgs (org_id));
diesel::joinable!(invitations -> users (invited_by));
diesel::joinable!(ip_addresses -> hosts (host_id));
diesel::joinable!(node_logs -> hosts (host_id));
diesel::joinable!(node_logs -> nodes (node_id));
diesel::joinable!(node_logs_old -> blockchains_old (blockchain_id));
diesel::joinable!(node_logs_old -> orgs (org_id));
diesel::joinable!(node_properties_old -> blockchain_properties_old (blockchain_property_id));
diesel::joinable!(node_properties_old -> nodes_old (node_id));
diesel::joinable!(node_reports -> nodes (node_id));
diesel::joinable!(nodes -> configs (config_id));
diesel::joinable!(nodes -> hosts (host_id));
diesel::joinable!(nodes -> images (image_id));
diesel::joinable!(nodes -> orgs (org_id));
diesel::joinable!(nodes -> protocol_versions (protocol_version_id));
diesel::joinable!(nodes -> protocols (protocol_id));
diesel::joinable!(nodes -> regions (scheduler_region_id));
diesel::joinable!(nodes_old -> blockchains_old (blockchain_id));
diesel::joinable!(nodes_old -> hosts_old (host_id));
diesel::joinable!(nodes_old -> orgs (org_id));
diesel::joinable!(nodes_old -> regions (scheduler_region));
diesel::joinable!(orgs -> addresses (address_id));
diesel::joinable!(protocol_versions -> orgs (org_id));
diesel::joinable!(protocols -> orgs (org_id));
diesel::joinable!(role_permissions -> permissions (permission));
diesel::joinable!(role_permissions -> roles (role));
diesel::joinable!(user_roles -> orgs (org_id));
diesel::joinable!(user_roles -> roles (role));
diesel::joinable!(user_roles -> users (user_id));
diesel::joinable!(user_settings -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    addresses,
    api_keys,
    archives,
    blockchain_node_types_old,
    blockchain_properties_old,
    blockchain_versions_old,
    blockchains_old,
    commands,
    configs,
    hosts,
    hosts_old,
    image_properties,
    image_rules,
    images,
    invitations,
    ip_addresses,
    node_logs,
    node_logs_old,
    node_properties_old,
    node_reports,
    nodes,
    nodes_old,
    orgs,
    permissions,
    protocol_versions,
    protocols,
    regions,
    role_permissions,
    roles,
    tokens,
    user_roles,
    user_settings,
    users,
);
