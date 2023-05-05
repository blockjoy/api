-- Your SQL goes here
CREATE INDEX idx_filter_nodes_to_upgrade ON nodes USING btree (node_type, string_to_array(lower(version),'.') DESC) WHERE self_update = true;

-- This name is with tbl to avoid conflict with the index name in init sql
CREATE INDEX idx_blockchains_tbl_name ON blockchains USING btree (lower(name));

