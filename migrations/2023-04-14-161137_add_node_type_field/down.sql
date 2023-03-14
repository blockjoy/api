-- This file should undo anything in `up.sql`

UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '0'::JSONB))  WHERE node_type = 'unknown'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '1'::JSONB))  WHERE node_type = 'miner'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '2'::JSONB))  WHERE node_type = 'etl'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '3'::JSONB))  WHERE node_type = 'validator'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '4'::JSONB))  WHERE node_type = 'rpc'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '5'::JSONB))  WHERE node_type = 'oracle'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '6'::JSONB))  WHERE node_type = 'relay'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '7'::JSONB))  WHERE node_type = 'executor'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '8'::JSONB))  WHERE node_type = 'beacon'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '9'::JSONB))  WHERE node_type = 'mevboost'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '10'::JSONB))  WHERE node_type = 'node'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '11'::JSONB))  WHERE node_type = 'full_node'::enum_node_type;
UPDATE nodes SET properties = (jsonb_set(properties, '{id}', '12'::JSONB))  WHERE node_type = 'light_node'::enum_node_type;

ALTER TABLE nodes DROP COLUMN node_type;
ALTER TABLE nodes RENAME properties TO node_type;

DROP TYPE enum_node_type;

CREATE TYPE enum_node_type AS ENUM (
    'undefined',
    'api',
    'etl',
    'node',
    'oracle',
    'relay',
    'validator',
    'miner'
);
