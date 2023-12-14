CREATE TYPE enum_node_chain_status AS ENUM (
    'unknown',
    'broadcasting',
    'cancelled',
    'delegating',
    'delinquent',
    'disabled',
    'earning',
    'electing',
    'elected',
    'exported',
    'ingesting',
    'mining',
    'minting',
    'processing',
    'relaying',
    'removed',
    'removing',
    'provisioning'
);
ALTER TABLE nodes RENAME COLUMN node_status TO chain_status;
ALTER TABLE nodes ALTER COLUMN chain_status DROP DEFAULT;
ALTER TABLE nodes ALTER COLUMN chain_status TYPE enum_node_chain_status USING chain_status::text::enum_node_chain_status;
ALTER TABLE nodes ALTER COLUMN chain_status SET DEFAULT 'unknown';
DROP TYPE enum_node_status;
ALTER TABLE nodes DROP COLUMN deleted_at;
ALTER TABLE hosts DROP COLUMN deleted_at;
ALTER TABLE commands ADD COLUMN sub_cmd TEXT NULL;
