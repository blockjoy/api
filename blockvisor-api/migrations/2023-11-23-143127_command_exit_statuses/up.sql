CREATE TYPE enum_command_exit_code AS ENUM (
    'ok',
    'internal_error',
    'node_not_found',
    'blocking_job_running',
    'service_not_ready',
    'service_broken',
    'not_supported'
);

ALTER TABLE commands RENAME COLUMN response TO exit_message;

ALTER TABLE commands ADD COLUMN retry_hint_seconds BIGINT NULL;

ALTER TABLE commands ADD COLUMN exit_code enum_command_exit_code NULL;
UPDATE commands SET exit_code = 'ok' WHERE exit_status = 0;
UPDATE commands SET exit_code = 'internal_error' WHERE exit_status = 1;
ALTER TABLE commands DROP COLUMN exit_status;
