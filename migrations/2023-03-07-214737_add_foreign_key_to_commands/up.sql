-- Your SQL goes here

-- Delete orphaned commands
DELETE FROM commands WHERE 
    (NOT EXISTS (SELECT 1 FROM nodes WHERE nodes.id = commands.resource_id)) AND
    (NOT EXISTS (SELECT 1 FROM hosts WHERE hosts.id = commands.resource_id));
ALTER TABLE commands ALTER COLUMN resource_id DROP NOT NULL;
ALTER TABLE commands RENAME COLUMN resource_id TO node_id;
ALTER TABLE commands ADD CONSTRAINT commands_node_id_fkey FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE;
