-- This file should undo anything in `up.sql`

ALTER TABLE nodes ALTER COLUMN blockchain_id DROP NOT NULL;
ALTER TABLE nodes ALTER COLUMN name DROP NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_by_user DROP NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_for_org DROP NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_at DROP NOT NULL;
ALTER TABLE orgs_users ALTER COLUMN role DROP NOT NULL;
    