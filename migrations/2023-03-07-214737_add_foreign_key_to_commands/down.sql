-- This file should undo anything in `up.sql`
ALTER TABLE commands RENAME COLUMN node_id TO resource_id;
ALTER TABLE commands DROP CONSTRAINT commands_node_id_fkey;
ALTER TABLE commands ALTER COLUMN resource_id SET NOT NULL;
