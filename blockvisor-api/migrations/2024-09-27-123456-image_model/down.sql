drop table node_logs;

drop index idx_commands_node_id;

alter table commands
    drop constraint fk_commands_host_id;

alter table commands
    drop constraint fk_commands_node_id;

alter table ip_addresses
    drop constraint fk_ip_addresses_host_id;

alter table node_reports
    drop constraint fk_node_reports_node_id;

drop table nodes;

drop table hosts;

drop table configs;

drop table archives;

drop table image_properties;

drop table image_rules;

drop table images;

drop table protocol_versions;

drop table protocols;

drop type enum_config_type;

drop type enum_node_state;

drop type enum_next_state;

drop type enum_health;

drop type enum_firewall_protocol;

drop type enum_firewall_direction;

drop type enum_firewall_action;

drop type enum_node_event;

drop type enum_ui_type;

alter table nodes_old rename to nodes;

alter index idx_nodes_old_org_id rename to idx_nodes_org_id;

alter index idx_nodes_old_host_id rename to idx_nodes_host_id;

drop index idx_node_reports_node_id;

alter table node_properties_old rename to node_properties;

alter table node_logs_old rename to node_logs;

alter table hosts_old rename to hosts;

alter type enum_host_type_old rename to enum_host_type;

alter type enum_connection_status rename to enum_conn_status;

alter type enum_schedule_type rename to enum_managed_by;

alter table blockchains_old rename to blockchains;

alter table blockchain_node_types_old rename to blockchain_node_types;

alter table blockchain_versions_old rename to blockchain_versions;

alter table blockchain_properties_old rename to blockchain_properties;

alter table user_settings rename column key to name;

alter type enum_visibility rename to enum_blockchain_visibility;

alter table ip_addresses
    add constraint ip_addresses_host_id_fkey foreign key (host_id) references hosts (id) on delete cascade;

alter table ip_addresses
    alter column host_id drop not null;

drop index idx_ip_addresses_host_id;

alter table tokens
    alter column org_id drop not null;

alter table tokens rename column created_by_type to created_by_resource;

alter table tokens rename column created_by_id to created_by;

create table subscriptions (
    id uuid primary key default uuid_generate_v4 (),
    org_id uuid unique not null,
    user_id uuid not null,
    external_id text not null,
    constraint fk_org_id foreign key (org_id) references orgs (id) on delete cascade,
    constraint fk_user_id foreign key (user_id) references users (id)
);

create index idx_subs_user_id on subscriptions using btree (user_id);

alter table users
    add column chargebee_billing_id text;

alter table commands
    add constraint fk_host_commands_hosts foreign key (host_id) references hosts (id);

alter table commands
    add constraint commands_node_id_fkey foreign key (node_id) references nodes (id);

alter table node_reports
    add constraint node_reports_node_id_fkey foreign key (node_id) references nodes (id);

alter table node_reports rename column created_by_type to created_by_resource;

alter table node_reports rename column created_by_id to created_by;
