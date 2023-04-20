ALTER TABLE nodes DROP COLUMN scheduler_similarity;
ALTER TABLE nodes DROP COLUMN scheduler_resource;

DROP TYPE enum_node_resource_affinity;
DROP TYPE enum_node_similarity_affinity;
