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
