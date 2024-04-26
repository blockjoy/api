ALTER TABLE blockchains
    DROP CONSTRAINT blockchain_name_key;
ALTER TABLE blockchain_node_types
    DROP CONSTRAINT blockchain_node_types_blockchain_id_node_type_key;
ALTER TABLE blockchain_versions
    DROP CONSTRAINT blockchain_versions_blockchain_node_type_id_version_key;
