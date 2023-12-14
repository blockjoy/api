CREATE TYPE enum_node_status AS ENUM (
  'provisioning_pending',
  'provisioning',
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
  'delete_pending',
  'deleting',
  'deleted',
  'update_pending',
  'updating'
);
ALTER TABLE nodes RENAME COLUMN chain_status TO node_status;
ALTER TABLE nodes ALTER COLUMN node_status DROP DEFAULT;
ALTER TABLE nodes ALTER COLUMN node_status TYPE enum_node_status USING node_status::text::enum_node_status;
DROP TYPE enum_node_chain_status;
ALTER TABLE nodes ADD COLUMN deleted_at TIMESTAMPTZ NULL;
ALTER TABLE hosts ADD COLUMN deleted_at TIMESTAMPTZ NULL;
ALTER TABLE commands DROP COLUMN sub_cmd;
