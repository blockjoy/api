
ALTER TYPE enum_node_status RENAME TO old_node_status;

CREATE TYPE enum_node_status AS ENUM (
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
    'mining',
    'minting',
    'processing',
    'provisioning',
    'provisioning_pending',
    'relaying',
    'unknown',
    'update_pending',
    'updating',
    'uploading'
);

ALTER TABLE nodes ADD column new_node_status enum_node_status NOT NULL DEFAULT 'unknown'::enum_node_status;
UPDATE nodes SET new_node_status = node_status::old_node_status::TEXT::enum_node_status;
ALTER TABLE nodes DROP column node_status;
ALTER TABLE nodes rename column new_node_status to node_status;
DROP TYPE old_node_status;
