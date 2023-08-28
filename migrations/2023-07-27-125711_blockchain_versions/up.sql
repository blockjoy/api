-- We are going to create a hierarchy of entities to describe a blockchain. A
-- record in the blockchains table represents a particular type of blockchain,
-- for example `ethereum`. Ethereum has multiple supported node types, which
-- are represented in the blockchain_node_types table. For each of those node
-- types, we then have a set of versions that exist. Each version then has an
-- associated set of properties.

-- First we create the two needed tables.
CREATE TABLE blockchain_node_types (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE RESTRICT,
    node_type enum_node_type NOT NULL,
    description TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE blockchain_versions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    blockchain_id UUID NOT NULL REFERENCES blockchains ON DELETE RESTRICT,
    blockchain_node_type_id UUID NOT NULL REFERENCES blockchain_node_types ON DELETE RESTRICT,
    version TEXT NOT NULL,
    description TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Here we populate the first two tables with the data that is currently still
-- in the blockchain_properties table.
INSERT INTO blockchain_node_types (blockchain_id, node_type)
SELECT DISTINCT
    blockchain_id, node_type
FROM
    blockchain_properties;

INSERT INTO blockchain_versions (blockchain_id, blockchain_node_type_id, version)
SELECT DISTINCT
    blockchain_properties.blockchain_id, blockchain_node_types.id, version
FROM
    blockchain_properties
INNER JOIN
    blockchain_node_types ON
        blockchain_properties.blockchain_id = blockchain_node_types.blockchain_id AND
        blockchain_properties.node_type = blockchain_node_types.node_type;

-- Now we will point each property to the correct blockchain_version record,
-- and then delete the `blockchain_id`, `version` and `node_type` columns from
-- it, which have become redundant.
ALTER TABLE blockchain_properties ADD COLUMN blockchain_node_type_id UUID NULL REFERENCES blockchain_node_types ON DELETE RESTRICT;
ALTER TABLE blockchain_properties ADD COLUMN blockchain_version_id UUID NULL REFERENCES blockchain_versions ON DELETE RESTRICT;
ALTER TABLE blockchain_properties ADD COLUMN display_name TEXT NULL;
UPDATE blockchain_properties SET
    display_name = name,
    blockchain_node_type_id = blockchain_node_types.id,
    blockchain_version_id = blockchain_versions.id
FROM
    blockchain_versions
INNER JOIN
    blockchain_node_types ON blockchain_node_types.id = blockchain_versions.blockchain_node_type_id
WHERE
    blockchain_node_types.blockchain_id = blockchain_properties.blockchain_id AND
    blockchain_node_types.node_type = blockchain_properties.node_type AND
    blockchain_versions.version = blockchain_properties.version;
ALTER TABLE blockchain_properties DROP COLUMN version;
ALTER TABLE blockchain_properties DROP COLUMN node_type;
ALTER TABLE blockchain_properties ALTER COLUMN blockchain_version_id SET NOT NULL;
ALTER TABLE blockchain_properties ALTER COLUMN blockchain_node_type_id SET NOT NULL;
ALTER TABLE blockchain_properties ALTER COLUMN display_name SET NOT NULL;

-- extra fix included for free
ALTER TYPE enum_node_type RENAME VALUE 'rpc' TO 'api';
