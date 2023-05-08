-- Your SQL goes here
CREATE INDEX idx_filter_nodes_to_upgrade ON nodes USING btree (node_type, string_to_array(lower(version),'.') DESC) WHERE self_update = true;

-- This name is with tbl to avoid conflict with the index name in init sql
CREATE INDEX idx_blockchains_tbl_name ON blockchains USING btree (lower(name));

-- WARN: Unfortunatelly, alter types are not supported in transactions and
-- we can't rollback if something goes wrong. So, we need to be careful here.
ALTER TYPE enum_host_cmd ADD VALUE IF NOT EXISTS 'upgrade_node';
