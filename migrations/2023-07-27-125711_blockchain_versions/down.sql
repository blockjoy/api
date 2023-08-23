ALTER TABLE blockchain_properties ADD COLUMN version TEXT NULL;
ALTER TABLE blockchain_properties ADD COLUMN node_type enum_node_type NULL;

UPDATE blockchain_properties SET
    blockchain_id = blockchain_versions.blockchain_id,
    version = blockchain_versions.version,
    node_type = blockchain_node_types.node_type
FROM blockchain_versions
INNER JOIN blockchain_node_types ON blockchain_node_types.id = blockchain_versions.blockchain_node_type_id
WHERE blockchain_versions.id = blockchain_version_id;

ALTER TABLE blockchain_properties ALTER COLUMN blockchain_id SET NOT NULL;
ALTER TABLE blockchain_properties ALTER COLUMN version SET NOT NULL;
ALTER TABLE blockchain_properties ALTER COLUMN node_type SET NOT NULL;

ALTER TABLE blockchain_properties DROP COLUMN blockchain_version_id;
ALTER TABLE blockchain_properties DROP COLUMN blockchain_node_type_id;
ALTER TABLE blockchain_properties DROP COLUMN display_name;

DROP TABLE blockchain_versions;
DROP TABLE blockchain_node_types;

ALTER TYPE enum_node_type RENAME VALUE 'api' TO 'rpc';
