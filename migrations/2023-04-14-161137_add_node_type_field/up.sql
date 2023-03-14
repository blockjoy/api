-- Your SQL goes here

DROP TYPE enum_node_type;

CREATE TYPE enum_node_type AS ENUM (
    'unknown',
    'miner',
    'etl',
    'validator',
    'rpc',
    'oracle',
    'relay',
    'executor',
    'beacon',
    'mevboost',
    'node',
    'full_node',
    'light_node'
);

ALTER TABLE nodes RENAME COLUMN node_type TO properties;

ALTER TABLE nodes ADD COLUMN node_type enum_node_type NULL;
UPDATE nodes SET node_type = 'unknown'::enum_node_type WHERE (properties->'id')::INTEGER = 0;
UPDATE nodes SET node_type = 'miner'::enum_node_type WHERE (properties->'id')::INTEGER = 1;
UPDATE nodes SET node_type = 'etl'::enum_node_type WHERE (properties->'id')::INTEGER = 2;
UPDATE nodes SET node_type = 'validator'::enum_node_type WHERE (properties->'id')::INTEGER = 3;
UPDATE nodes SET node_type = 'rpc'::enum_node_type WHERE (properties->'id')::INTEGER = 4;
UPDATE nodes SET node_type = 'oracle'::enum_node_type WHERE (properties->'id')::INTEGER = 5;
UPDATE nodes SET node_type = 'relay'::enum_node_type WHERE (properties->'id')::INTEGER = 6;
UPDATE nodes SET node_type = 'executor'::enum_node_type WHERE (properties->'id')::INTEGER = 7;
UPDATE nodes SET node_type = 'beacon'::enum_node_type WHERE (properties->'id')::INTEGER = 8;
UPDATE nodes SET node_type = 'mevboost'::enum_node_type WHERE (properties->'id')::INTEGER = 9;
UPDATE nodes SET node_type = 'node'::enum_node_type WHERE (properties->'id')::INTEGER = 10;
UPDATE nodes SET node_type = 'full_node'::enum_node_type WHERE (properties->'id')::INTEGER = 11;
UPDATE nodes SET node_type = 'light_node'::enum_node_type WHERE (properties->'id')::INTEGER = 12;
ALTER TABLE nodes ALTER COLUMN node_type SET NOT NULL;

UPDATE nodes SET properties = properties - 'id';

CREATE TABLE blockchain_node_properties (
    id SERIAL PRIMARY KEY,
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE CASCADE,
    version VARCHAR(32) NOT NULL,
    node_type enum_node_type NOT NULL,
    name VARCHAR(64) NOT NULL,
    description VARCHAR(512) NOT NULL,
    ui_type VARCHAR(32) NOT NULL,
    disabled BOOLEAN NOT NULL,
    required BOOLEAN NOT NULL
);

INSERT INTO blockchain_node_properties (blockchain_id, version, node_type, name, label, description, ui_type, disabled, required, value)
    SELECT (
        id,
        supported_node_types->'version',
        CASE supported_node_types->'properties'->'id'::INTEGER 
            WHEN 0 THEN 'unknown'::enum_node_type
            WHEN 1 THEN 'miner'::enum_node_type
            WHEN 2 THEN 'etl'::enum_node_type
            WHEN 3 THEN 'validator'::enum_node_type
            WHEN 4 THEN 'rpc'::enum_node_type
            WHEN 5 THEN 'oracle'::enum_node_type
            WHEN 6 THEN 'relay'::enum_node_type
            WHEN 7 THEN 'executor'::enum_node_type
            WHEN 8 THEN 'beacon'::enum_node_type
            WHEN 9 THEN 'mevboost'::enum_node_type
            WHEN 10 THEN 'node'::enum_node_type
            WHEN 11 THEN 'full_node'::enum_node_type
            WHEN 12 THEN 'light_node'::enum_node_type
        END,
        supported_node_types->'properties'->'name',
        supported_node_types->'properties'->'description',
        supported_node_types->'properties'->'default',
        supported_node_types->'properties'->'ui_type',
        supported_node_types->'properties'->'disabled',
        supported_node_types->'properties'->'required'
    ) FROM blockchains;

-- [
--     {
--         "id": 3,
--         "version": "1.17.2-build.5",
--         "properties": [
--             {
--                 "name": "self-hosted",
--                 "default": "false",
--                 "ui_type": "switch",
--                 "disabled": true,
--                 "required": true
--             }
--         ]
--     }
-- ]