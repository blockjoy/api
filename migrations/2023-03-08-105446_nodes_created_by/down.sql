-- This file should undo anything in `up.sql`

ALTER TABLE nodes DROP COLUMN created_by;
