-- old tables will remain until migration is successful
alter table blockchains rename to blockchains_old;

alter table blockchain_node_types rename to blockchain_node_types_old;

alter table blockchain_versions rename to blockchain_versions_old;

alter table blockchain_properties rename to blockchain_properties_old;

alter table hosts rename to hosts_old;

alter table nodes rename to nodes_old;

alter index idx_nodes_org_id rename to idx_nodes_old_org_id;

alter index idx_nodes_host_id rename to idx_nodes_old_host_id;

alter table node_properties rename to node_properties_old;

alter table node_logs rename to node_logs_old;

alter table user_settings rename column name to key;

alter type enum_blockchain_visibility rename to enum_visibility;

alter type enum_conn_status rename to enum_connection_status;

alter type enum_host_type rename to enum_host_type_old;

alter type enum_managed_by rename to enum_schedule_type;

create type enum_config_type as enum (
  'legacy',
  'node'
);

create type enum_node_state as enum (
  'deleted',
  'deleting',
  'failed',
  'running',
  'starting',
  'stopped',
  'upgrading'
);

create type enum_next_state as enum (
  'deleting',
  'stopping',
  'upgrading'
);

create type enum_health as enum (
  'healthy',
  'neutral',
  'unhealthy'
);

create type enum_firewall_protocol as enum (
  'both',
  'tcp',
  'udp'
);

create type enum_firewall_direction as enum (
  'inbound',
  'outbound'
);

create type enum_firewall_action as enum (
  'allow',
  'drop',
  'reject'
);

create type enum_node_event as enum (
  'config_updated',
  'create_cancelled',
  'create_failed',
  'create_started',
  'create_succeeded',
  'org_transferred',
  'upgrade_failed',
  'upgrade_started',
  'upgrade_succeeded'
);

create type enum_ui_type as enum (
  'enum',
  'password',
  'switch',
  'text'
);

create table protocols (
  id uuid primary key default uuid_generate_v4 (),
  org_id uuid references orgs (id),
  key text not null unique,
  name text not null,
  description text,
  ticker text,
  visibility enum_visibility not null default 'private' ::enum_visibility,
  created_at timestamp with time zone default now() not null,
  updated_at timestamp with time zone
);

create index idx_protocols_org_id on protocols using btree (org_id);

create table protocol_versions (
  id uuid primary key default uuid_generate_v4 (),
  org_id uuid references orgs (id),
  protocol_id uuid not null references protocols (id),
  protocol_key text not null references protocols (key),
  variant_key text not null,
  semantic_version text not null,
  sku_code text not null,
  description text,
  visibility enum_visibility not null default 'private' ::enum_visibility,
  created_at timestamp with time zone default now() not null,
  updated_at timestamp with time zone,
  unique (protocol_key, variant_key, semantic_version)
);

create index idx_protocol_versions_org_id on protocol_versions using btree (org_id);

create index idx_protocol_versions_protocol_id on protocol_versions using btree (protocol_id);

create index idx_protocol_versions_version_key on protocol_versions using btree (protocol_key, variant_key);

create table images (
  id uuid primary key default uuid_generate_v4 (),
  org_id uuid references orgs (id),
  protocol_version_id uuid not null references protocol_versions (id),
  image_uri text not null,
  build_version bigint not null,
  description text,
  min_cpu_cores bigint not null,
  min_memory_bytes bigint not null,
  min_disk_bytes bigint not null,
  ramdisks jsonb not null default '[]',
  default_firewall_in enum_firewall_action not null,
  default_firewall_out enum_firewall_action not null,
  visibility enum_visibility not null default 'private' ::enum_visibility,
  created_at timestamp with time zone default now() not null,
  updated_at timestamp with time zone,
  unique (protocol_version_id, build_version)
);

create index idx_images_org_id on images using btree (org_id);

create index idx_images_protocol_version_id on images using btree (protocol_version_id, build_version);

create table image_properties (
  id uuid primary key default uuid_generate_v4 (),
  image_id uuid not null references images (id),
  key text not null,
  key_group text,
  is_group_default boolean,
  new_archive boolean not null,
  default_value text not null,
  dynamic_value boolean not null default false,
  description text,
  ui_type enum_ui_type not null,
  add_cpu_cores bigint,
  add_memory_bytes bigint,
  add_disk_bytes bigint,
  unique (image_id, key)
);

create index idx_image_properties_image_id on image_properties using btree (image_id, key);

create table image_rules (
  id uuid primary key default uuid_generate_v4 (),
  image_id uuid not null references images (id),
  key text not null,
  description text,
  protocol enum_firewall_protocol not null,
  direction enum_firewall_direction not null,
  action enum_firewall_action not null,
  ips jsonb,
  ports jsonb
);

create index idx_image_rules_image_id on image_rules using btree (image_id);

create table archives (
  id uuid primary key default uuid_generate_v4 (),
  org_id uuid references orgs (id),
  image_id uuid not null references images (id),
  store_id text not null,
  image_property_ids uuid[] not null,
  unique (image_id, image_property_ids)
);

create index idx_archives_org_id on archives using btree (org_id);

create index idx_archives_image_id on archives using btree (image_id);

create index idx_archives_store_id on archives using btree (store_id);

create index idx_archives_image_property_ids on archives using gin (image_property_ids);

create table configs (
  id uuid primary key default uuid_generate_v4 (),
  image_id uuid not null references images (id),
  archive_id uuid not null references archives (id),
  config_type enum_config_type not null,
  config bytea not null,
  created_by_type enum_resource_type not null,
  created_by_id uuid not null,
  created_at timestamp with time zone default now() not null
);

create index idx_configs_image_id on configs using btree (image_id);

create index idx_configs_archive_id on configs using btree (archive_id);

create table hosts (
  id uuid primary key default uuid_generate_v4 (),
  org_id uuid references orgs (id),
  region_id uuid references regions (id) on delete set null,
  network_name text not null unique,
  display_name text,
  schedule_type enum_schedule_type not null default 'automatic' ::enum_schedule_type,
  connection_status enum_connection_status not null default 'offline' ::enum_connection_status,
  cpu_cores bigint not null,
  memory_bytes bigint not null,
  disk_bytes bigint not null,
  os text not null,
  os_version text not null,
  bv_version text not null,
  ip_address inet not null,
  ip_gateway inet not null,
  node_count bigint not null default 0,
  node_cpu_cores bigint not null default 0,
  node_memory_bytes bigint not null default 0,
  node_disk_bytes bigint not null default 0,
  used_cpu_hundreths bigint,
  used_memory_bytes bigint,
  used_disk_bytes bigint,
  load_one_percent double precision,
  load_five_percent double precision,
  load_fifteen_percent double precision,
  network_received_bytes bigint,
  network_sent_bytes bigint,
  uptime_seconds bigint,
  tags text[] not null default '{}' ::text[],
  created_by_type enum_resource_type not null,
  created_by_id uuid not null,
  created_at timestamp with time zone default now() not null,
  updated_at timestamp with time zone,
  deleted_at timestamp with time zone
);

create index idx_hosts_org_id on hosts using btree (org_id);

create index idx_hosts_region_id on hosts using btree (region_id);

alter table ip_addresses
  drop constraint ip_addresses_host_id_fkey;

alter table ip_addresses
  add constraint fk_ip_addresses_host_id foreign key (host_id) references hosts (id) on delete cascade;

alter table ip_addresses
  alter column host_id set not null;

create index idx_ip_addresses_host_id on ip_addresses using btree (host_id);

create table nodes (
  id uuid primary key default uuid_generate_v4 (),
  node_name text not null unique,
  display_name text,
  old_node_id uuid,
  org_id uuid not null references orgs (id),
  host_id uuid not null references hosts (id),
  image_id uuid not null references images (id),
  config_id uuid not null references configs (id),
  protocol_id uuid not null references protocols (id),
  protocol_version_id uuid not null references protocol_versions (id),
  semantic_version text not null,
  auto_upgrade boolean not null,
  node_state enum_node_state not null,
  next_state enum_next_state default null,
  protocol_state text default null,
  protocol_health enum_health default null,
  jobs jsonb,
  note text,
  tags text[] not null default '{}',
  ip_address inet not null,
  ip_gateway inet not null,
  p2p_address text,
  dns_id text not null,
  dns_name text not null,
  dns_url text,
  cpu_cores bigint not null,
  memory_bytes bigint not null,
  disk_bytes bigint not null,
  block_height bigint,
  block_age bigint,
  consensus boolean,
  scheduler_similarity enum_node_similarity_affinity,
  scheduler_resource enum_node_resource_affinity,
  scheduler_region_id uuid references regions (id),
  stripe_item_id text,
  created_by_type enum_resource_type not null,
  created_by_id uuid not null,
  created_at timestamp with time zone default now() not null,
  updated_at timestamp with time zone,
  deleted_at timestamp with time zone
);

create index idx_nodes_org_id on nodes using btree (org_id);

create index idx_nodes_host_id on nodes using btree (host_id);

create index idx_nodes_image_id on nodes using btree (image_id);

create index idx_nodes_config_id on nodes using btree (config_id);

create index idx_nodes_protocol_id on nodes using btree (protocol_id);

create index idx_nodes_protocol_version_id on nodes using btree (protocol_version_id);

create index idx_nodes_scheduler_region_id on nodes using btree (scheduler_region_id);

drop index idx_commands_host_id;

alter table commands
  drop constraint commands_node_id_fkey;

alter table commands
  drop constraint fk_host_commands_hosts;

alter table commands
  add constraint fk_commands_host_id foreign key (host_id) references hosts (id) on delete cascade;

alter table commands
  add constraint fk_commands_node_id foreign key (node_id) references nodes (id);

create index idx_commands_host_id on commands using btree (host_id);

create index idx_commands_node_id on commands using btree (node_id);

alter table node_reports
  drop constraint node_reports_node_id_fkey;

alter table node_reports
  add constraint fk_node_reports_node_id foreign key (node_id) references nodes (id);

alter table node_reports rename column created_by_resource to created_by_type;

alter table node_reports rename column created_by to created_by_id;

create index idx_node_reports_node_id on node_reports using btree (node_id);

create table node_logs (
  id uuid primary key default uuid_generate_v4 (),
  node_id uuid not null references nodes (id),
  host_id uuid not null references hosts (id),
  event enum_node_event not null,
  event_data jsonb,
  created_by_type enum_resource_type not null,
  created_by_id uuid not null,
  created_at timestamp with time zone default now() not null
);

create index idx_nodes_logs_node_id on node_logs using btree (node_id);

create index idx_nodes_logs_host_id on node_logs using btree (host_id);

alter table tokens
  alter column org_id set not null;

alter table tokens rename column created_by_resource to created_by_type;

alter table tokens rename column created_by to created_by_id;

drop table subscriptions;

alter table users
  drop column chargebee_billing_id;

-- The nil uuid is used to migrate legacy data
insert into protocols (id, key, name)
select
  uuid_nil (),
  'legacy',
  'legacy';

insert into protocol_versions (id, protocol_id, protocol_key, variant_key, semantic_version, sku_code)
select
  uuid_nil (),
  uuid_nil (),
  'legacy',
  'legacy',
  '0.0.0',
  'legacy';

insert into images (id, protocol_version_id, image_uri, build_version, min_cpu_cores, min_memory_bytes, min_disk_bytes, default_firewall_in, default_firewall_out)
select
  uuid_nil (),
  uuid_nil (),
  'legacy',
  0,
  0,
  0,
  0,
  'drop'::enum_firewall_action,
  'allow'::enum_firewall_action;

insert into archives (id, image_id, store_id, image_property_ids)
select
  uuid_nil (),
  uuid_nil (),
  'legacy',
  array[]::uuid[];

insert into configs (id, image_id, archive_id, config_type, config, created_by_type, created_by_id)
select
  uuid_nil (),
  uuid_nil (),
  uuid_nil (),
  'legacy'::enum_config_type,
  ''::bytea,
  'user'::enum_resource_type,
  uuid_nil ();

insert into hosts (id, org_id, region_id, network_name, schedule_type, connection_status, cpu_cores, memory_bytes, disk_bytes, os, os_version, bv_version, ip_address, ip_gateway, tags, created_by_type, created_by_id, created_at, deleted_at)
select
  id,
  case when host_type = 'private'::enum_host_type_old then
    org_id
  else
    null
  end,
  region_id,
  name,
  managed_by,
  status,
  cpu_count,
  mem_size_bytes,
  disk_size_bytes,
  os,
  os_version,
  version,
  ip_addr::inet,
  ip_gateway,
  tags,
  'user'::enum_resource_type,
  created_by,
  created_at,
  deleted_at
from
  hosts_old;

insert into nodes (id, node_name, display_name, old_node_id, org_id, host_id, image_id, config_id, protocol_id, protocol_version_id, semantic_version, auto_upgrade, node_state, jobs, note, tags, ip_address, ip_gateway, p2p_address, dns_id, dns_name, cpu_cores, memory_bytes, disk_bytes, block_height, block_age, consensus, scheduler_similarity, scheduler_resource, scheduler_region_id, stripe_item_id, created_by_type, created_by_id, created_at, updated_at, deleted_at)
select
  id,
  node_name,
  display_name,
  old_node_id,
  org_id,
  host_id,
  uuid_nil (),
  uuid_nil (),
  uuid_nil (),
  uuid_nil (),
  version,
  self_update,
  'upgrading'::enum_node_state,
  jobs,
  note,
  tags,
  ip,
  ip_gateway::inet,
  wallet_address,
  dns_record_id,
  dns_name,
  vcpu_count,
  mem_size_bytes,
  disk_size_bytes,
  block_height,
  block_age,
  consensus,
  scheduler_similarity,
  scheduler_resource,
  scheduler_region,
  stripe_item_id,
  created_by_resource,
  created_by,
  created_at,
  updated_at,
  deleted_at
from
  nodes_old;
