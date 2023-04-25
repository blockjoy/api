-- This file should undo anything in `up.sql`

ALTER TABLE nodes ALTER COLUMN scheduler_resource SET NOT NULL;

ALTER TABLE hosts DROP COLUMN org_id;

ALTER TABLE host_provisions DROP COLUMN org_id;
