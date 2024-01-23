create type enum_host_cmd as enum (
  'create_bvs',
  'create_node',
  'delete_node',
  'get_bvs_version',
  'get_node_version',
  'kill_node',
  'migrate_node',
  'remove_bvs',
  'restart_bvs',
  'restart_node',
  'shutdown_node',
  'stop_bvs',
  'update_bvs',
  'update_node',
  'upgrade_node'
);

alter table commands add column cmd enum_host_cmd;

update commands set cmd = 'create_node'
where command_type = 'node_create'::enum_command_type;

update commands set cmd = 'shutdown_node'
where command_type = 'node_stop'::enum_command_type;

update commands set cmd = 'restart_node'
where command_type = 'node_restart'::enum_command_type;

update commands set cmd = 'delete_node'
where command_type = 'node_delete'::enum_command_type;

update commands set cmd = 'update_node'
where command_type = 'node_update'::enum_command_type;

update commands set cmd = 'upgrade_node'
where command_type = 'node_upgrade'::enum_command_type;

update commands set cmd = 'restart_bvs'
where command_type = 'host_restart'::enum_command_type;

update commands set cmd = 'stop_bvs'
where command_type = 'host_stop'::enum_command_type;

update commands set cmd = 'create_bvs'
where command_type = 'host_start'::enum_command_type;

update commands set cmd = 'update_bvs'
where command_type = 'host_pending'::enum_command_type;

alter table commands alter column cmd set not null;

alter table commands drop column command_type;
drop type enum_command_type;
