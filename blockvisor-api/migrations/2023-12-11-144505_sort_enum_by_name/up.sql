-- enum_command_exit_code

alter type enum_command_exit_code rename to old_command_exit_code;

create type enum_command_exit_code as enum (
'blocking_job_running',
'internal_error',
'node_not_found',
'not_supported',
'ok',
'service_broken',
'service_not_ready'
);

alter table commands add column new_exit_code enum_command_exit_code;
update commands set new_exit_code = exit_code::old_command_exit_code::text::enum_command_exit_code;

alter table commands drop column exit_code;
alter table commands rename column new_exit_code to exit_code;

drop type old_command_exit_code;

-- enum_node_type

alter type enum_node_type rename to old_node_type;

create type enum_node_type as enum (
'api',
'archive',
'beacon',
'etl',
'executor',
'full_node',
'light_node',
'mevboost',
'miner',
'node',
'oracle',
'relay',
'unknown',
'validator'
);

alter table nodes add column new_node_type enum_node_type
not null default 'unknown'::enum_node_type;
update nodes set new_node_type = node_type::old_node_type::text::enum_node_type;

alter table nodes drop column node_type;
alter table nodes rename column new_node_type to node_type;

alter table node_logs add column new_node_type enum_node_type
not null default 'unknown'::enum_node_type;
update node_logs set new_node_type = node_type::old_node_type::text::enum_node_type;

alter table node_logs drop column node_type;
alter table node_logs rename column new_node_type to node_type;

alter table blockchain_node_types add column new_node_type enum_node_type
not null default 'unknown'::enum_node_type;
update blockchain_node_types set new_node_type = node_type::old_node_type::text::enum_node_type;

alter table blockchain_node_types drop column node_type;
alter table blockchain_node_types rename column new_node_type to node_type;

drop type old_node_type;

-- enum_node_status

alter type enum_node_status rename to old_node_status;

create type enum_node_status as enum (
'broadcasting',
'cancelled',
'delegating',
'delete_pending',
'deleted',
'deleting',
'delinquent',
'disabled',
'earning',
'elected',
'electing',
'exported',
'ingesting',
'mining',
'minting',
'processing',
'provisioning',
'provisioning_pending',
'relaying',
'unknown',
'update_pending',
'updating'
);

alter table nodes add column new_node_status enum_node_status
not null default 'unknown'::enum_node_status;
update nodes set new_node_status = node_status::old_node_status::text::enum_node_status;

alter table nodes drop column node_status;
alter table nodes rename column new_node_status to node_status;

drop type old_node_status;

-- enum_container_status

alter type enum_container_status rename to old_container_status;

create type enum_container_status as enum (
'creating',
'deleted',
'deleting',
'failed',
'installing',
'running',
'snapshotting',
'starting',
'stopped',
'stopping',
'unknown',
'upgraded',
'upgrading'
);

alter table nodes add column new_container_status enum_container_status
not null default 'unknown'::enum_container_status;
update nodes set new_container_status = container_status::old_container_status::text::enum_container_status;

alter table nodes drop column container_status;
alter table nodes rename column new_container_status to container_status;

drop type old_container_status;

-- enum_node_sync_status

alter type enum_node_sync_status rename to old_node_sync_status;

create type enum_node_sync_status as enum (
'synced',
'syncing',
'unknown'
);

alter table nodes add column new_node_sync_status enum_node_sync_status
not null default 'unknown'::enum_node_sync_status;
update nodes set new_node_sync_status = sync_status::old_node_sync_status::text::enum_node_sync_status;

alter table nodes drop column sync_status;
alter table nodes rename column new_node_sync_status to sync_status;

drop type old_node_sync_status;

-- enum_node_staking_status

alter type enum_node_staking_status rename to old_node_staking_status;

create type enum_node_staking_status as enum (
'consensus',
'follower',
'staked',
'staking',
'unknown',
'unstaked',
'validating'
);

alter table nodes add column new_node_staking_status enum_node_staking_status
default 'unknown'::enum_node_staking_status;
update nodes set new_node_staking_status = staking_status::old_node_staking_status::text::enum_node_staking_status;

alter table nodes drop column staking_status;
alter table nodes rename column new_node_staking_status to staking_status;

drop type old_node_staking_status;
