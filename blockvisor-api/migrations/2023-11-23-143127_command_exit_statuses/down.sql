ALTER TABLE commands RENAME COLUMN exit_message TO response;

ALTER TABLE commands DROP COLUMN retry_hint_seconds;

ALTER TABLE commands ADD COLUMN exit_status BIGINT NULL;
UPDATE commands SET exit_status = 0 WHERE exit_code = 'ok';
UPDATE commands SET exit_status = 1 WHERE exit_code = 'internal_error';
ALTER TABLE commands DROP COLUMN exit_code;

DROP TYPE enum_command_exit_code;
