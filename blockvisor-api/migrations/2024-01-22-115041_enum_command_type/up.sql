create type enum_command_type as enum (
  'host_pending',
  'host_restart',
  'host_start',
  'host_stop',
  'node_create',
  'node_delete',
  'node_restart',
  'node_start',
  'node_stop',
  'node_update',
  'node_upgrade'
);

alter table commands add column command_type enum_command_type;

update commands set command_type = 'node_create'
where cmd = 'create_node'::enum_host_cmd;

update commands set command_type = 'node_stop'
where cmd in ('kill_node'::enum_host_cmd, 'shutdown_node'::enum_host_cmd);

update commands set command_type = 'node_restart'
where cmd = 'restart_node'::enum_host_cmd;

update commands set command_type = 'node_delete'
where cmd = 'delete_node'::enum_host_cmd;

update commands set command_type = 'node_update'
where cmd = 'update_node'::enum_host_cmd;

update commands set command_type = 'node_upgrade'
where cmd = 'upgrade_node'::enum_host_cmd;

update commands set command_type = 'host_restart'
where cmd = 'restart_bvs'::enum_host_cmd;

update commands set command_type = 'host_stop'
where cmd in ('stop_bvs'::enum_host_cmd, 'remove_bvs'::enum_host_cmd);

update commands set command_type = 'host_start'
where cmd = 'create_bvs'::enum_host_cmd;

-- all other commands use a signal trigger to request pending commands
update commands set command_type = 'host_pending'
where cmd in (
  'update_bvs'::enum_host_cmd,
  'get_bvs_version'::enum_host_cmd,
  'get_node_version'::enum_host_cmd,
  'migrate_node'::enum_host_cmd
);

alter table commands alter column command_type set not null;

alter table commands drop column cmd;
drop type enum_host_cmd;
