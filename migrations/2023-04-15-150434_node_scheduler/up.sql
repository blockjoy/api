CREATE TYPE enum_node_similarity_affinity AS ENUM (
    'cluster',
    'spread'
);

CREATE TYPE enum_node_resource_affinity AS ENUM (
    'most_resources',
    'least_resources'
);

ALTER TABLE nodes ADD COLUMN scheduler_similarity enum_node_similarity_affinity NULL;
ALTER TABLE nodes ADD COLUMN scheduler_resource enum_node_resource_affinity NULL;
UPDATE nodes SET scheduler_resource = 'least_resources';
ALTER TABLE nodes ALTER COLUMN scheduler_resource SET NOT NULL;
