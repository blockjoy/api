CREATE TABLE IF NOT EXISTS node_types (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4() NOT NULL,
    name VARCHAR(128) UNIQUE
);

CREATE TABLE IF NOT EXISTS node_type_requirements (
    node_type_id UUID NOT NULL REFERENCES node_types ON DELETE CASCADE,
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE CASCADE,
    cpu_cores_required int,
    ram_required int,
    hdd_required int,
    PRIMARY KEY (node_type_id, blockchain_id)
);

CREATE TYPE enum_node_property_field_type AS ENUM ('file-upload', 'bool', 'text', 'ip', 'number');

CREATE TABLE IF NOT EXISTS node_type_properties (
    node_type_id UUID NOT NULL REFERENCES node_types ON DELETE CASCADE,
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE CASCADE,
    name varchar(128) not null,
    field_type enum_node_property_field_type not null,
    default_value text,
    label text,
    PRIMARY KEY (node_type_id, blockchain_id)
);

CREATE TABLE IF NOT EXISTS node_type_settings (
    node_id UUID NOT NULL REFERENCES nodes ON DELETE CASCADE,
    node_type_properties_id UUID NOT NULL REFERENCES node_type_properties ON DELETE CASCADE,
    value text,
    PRIMARY KEY (node_id, node_type_properties_id)
);
