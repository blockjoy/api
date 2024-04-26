ALTER TABLE blockchains
    ADD CONSTRAINT blockchain_name_key UNIQUE (name);
ALTER TABLE blockchain_node_types
    ADD CONSTRAINT blockchain_node_types_blockchain_id_node_type_key UNIQUE (blockchain_id, node_type);
ALTER TABLE blockchain_versions
    ADD CONSTRAINT blockchain_versions_blockchain_node_type_id_version_key UNIQUE (blockchain_node_type_id, version);
