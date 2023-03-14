-- This file should undo anything in `up.sql`

ALTER TABLE commands ALTER COLUMN node_id SET DEFAULT uuid_generate_v4();
