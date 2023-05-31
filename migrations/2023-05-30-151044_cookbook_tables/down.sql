-- This file should undo anything in `up.sql`

ALTER TABLE nodes ADD COLUMN properties JSONB DEFAULT '[]'::JSONB NOT NULL;
DROP TABLE node_properties;

ALTER TABLE blockchains ADD COLUMN supported_node_types JSONB DEFAULT '[]'::JSONB NOT NULL;
DROP TABLE blockchain_properties;
DROP TYPE blockchain_property_ui_type;
