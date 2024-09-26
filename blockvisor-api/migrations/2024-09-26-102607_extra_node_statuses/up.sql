ALTER TYPE enum_node_status RENAME TO enum_node_status_old;

CREATE TYPE enum_node_status AS ENUM(
    'active',
    'broadcasting',
    'cancelled',
    'delegating',
    'delete_pending',
    'deleted',
    'deleting',
    'delinquent',
    'disabled',
    'downloading',
    'earning',
    'elected',
    'electing',
    'exported',
    'ingesting',
    'initializing',
    'jailed',
    'mining',
    'minting',
    'processing',
    'provisioning',
    'provisioning_pending',
    'relaying',
    'starting',
    'unknown',
    'update_pending',
    'updating',
    'uploading'
);

ALTER TABLE nodes ALTER COLUMN node_status DROP DEFAULT;
ALTER TABLE nodes ALTER COLUMN node_status TYPE enum_node_status USING node_status::text::enum_node_status;
ALTER TABLE nodes ALTER COLUMN node_status SET DEFAULT 'unknown';

DROP TYPE enum_node_status_old;
