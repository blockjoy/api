-- Your SQL goes here

DROP TYPE enum_node_type;

CREATE TYPE enum_node_type AS ENUM (
    'unknown',
    'miner',
    'etl',
    'validator',
    'api',
    'oracle',
    'relay',
    'execution',
    'beacon',
    'mevboost',
    'node',
    'full_node',
    'light_node'
);

ALTER TABLE nodes ALTER COLUMN node_type RENAME TO properties;

ALTER TABLE nodes ADD COLUMN node_type enum_node_type NULL;
UPDATE nodes SET node_type = 'unknown'::enum_node_type WHERE properties->'id' = 0;
UPDATE nodes SET node_type = 'miner'::enum_node_type WHERE properties->'id' = 1;
UPDATE nodes SET node_type = 'etl'::enum_node_type WHERE properties->'id' = 2;
UPDATE nodes SET node_type = 'validator'::enum_node_type WHERE properties->'id' = 3;
UPDATE nodes SET node_type = 'api'::enum_node_type WHERE properties->'id' = 4;
UPDATE nodes SET node_type = 'oracle'::enum_node_type WHERE properties->'id' = 5;
UPDATE nodes SET node_type = 'relay'::enum_node_type WHERE properties->'id' = 6;
UPDATE nodes SET node_type = 'execution'::enum_node_type WHERE properties->'id' = 7;
UPDATE nodes SET node_type = 'beacon'::enum_node_type WHERE properties->'id' = 8;
UPDATE nodes SET node_type = 'mevboost'::enum_node_type WHERE properties->'id' = 9;
UPDATE nodes SET node_type = 'node'::enum_node_type WHERE properties->'id' = 10;
UPDATE nodes SET node_type = 'full_node'::enum_node_type WHERE properties->'id' = 11;
UPDATE nodes SET node_type = 'light_node'::enum_node_type WHERE properties->'id' = 12;
ALTER TABLE node_type SET NOT NULL;

UPDATE nodes SET properties = properties - 'id';
