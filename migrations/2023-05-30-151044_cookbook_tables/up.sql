-- Your SQL goes here

CREATE TYPE blockchain_property_ui_type AS ENUM (
    'switch',
    'password',
    'text',
    'file_upload'
);

CREATE TABLE blockchain_properties (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE CASCADE,
    version TEXT NOT NULL,
    node_type enum_node_type NOT NULL,
    name TEXT NOT NULL,
    "default" TEXT NULL,
    ui_type blockchain_property_ui_type NOT NULL,
    disabled BOOLEAN NOT NULL,
    required BOOLEAN NOT NULL
);

INSERT INTO blockchain_properties(blockchain_id, version, node_type, name, "default", ui_type, disabled, required) 
SELECT 
    blockchain_id, version, node_type, name, "default", ui_type, disabled, required
FROM (
    SELECT
        id AS blockchain_id,
        CASE (node_types->>'id')::INT
            WHEN 0  THEN 'undefined'
            WHEN 4  THEN       'api'
            WHEN 2  THEN       'etl'
            WHEN 10 THEN      'node'
            WHEN 5  THEN    'oracle'
            WHEN 6  THEN     'relay'
            WHEN 3  THEN 'validator'
            WHEN 1  THEN     'miner'
        END::enum_node_type AS node_type,
        node_types->>'version' AS version,
        node_type->>'name' AS name,
        node_type->>'default' AS "default",
        CASE node_type->>'ui_type'
            WHEN 'switch'         THEN 'switch'
            WHEN 'password'       THEN 'password'
            WHEN 'voting_key_pwd' THEN 'password'
            WHEN 'text'           THEN 'text'
            WHEN 'wallet_address' THEN 'text'
            WHEN 'file_upload'    THEN 'file_upload'
            WHEN 'key-upload'     THEN 'file_upload'
        END::blockchain_property_ui_type AS ui_type,
        cast(node_type->>'disabled' AS BOOLEAN) AS disabled,
        cast(node_type->>'required' AS BOOLEAN) AS required
    FROM blockchains,
         jsonb_array_elements(supported_node_types) AS node_types,
         jsonb_array_elements(node_types->'properties') AS node_type
) AS subkweery;

ALTER TABLE blockchains DROP COLUMN supported_node_types;



CREATE TABLE node_properties (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    node_id UUID NOT NULL REFERENCES nodes ON DELETE CASCADE,
    blockchain_property_id UUID NOT NULL REFERENCES blockchain_properties ON DELETE CASCADE,
    value TEXT NOT NULL
);

INSERT INTO node_properties(node_id, blockchain_property_id, value)
SELECT
    node_id, blockchain_property_id, value
FROM (
    SELECT
        subkweery.id AS node_id,
        blockchain_properties.id AS blockchain_property_id,
        properties_inner->>'value' AS value
    FROM (
        SELECT
            nodes.*,
            jsonb_array_elements(properties->'properties') AS properties_inner
        FROM nodes
    ) AS subkweery
    INNER JOIN
        blockchain_properties ON
            blockchain_properties.version = properties->>'version' AND
            blockchain_properties.name = properties_inner->>'name'
) AS subkweery;

ALTER TABLE nodes DROP COLUMN properties;
