CREATE TYPE enum_node_similarity_affinity AS ENUM (
    'cluster',
    'spread'
);

CREATE TYPE enum_node_resource_affinity AS ENUM (
    'most_resources',
    'least_resources'
);

CREATE TABLE node_schedulers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    node_id UUID NOT NULL REFERENCES nodes ON DELETE CASCADE,
    similarity enum_node_similarity_affinity NULL,
    resource enum_node_resource_affinity NOT NULL
);
