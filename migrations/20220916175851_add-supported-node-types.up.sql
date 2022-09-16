ALTER TABLE blockchains ADD COLUMN supported_node_types enum_node_type[] NOT NULL DEFAULT '{"undefined"}';
