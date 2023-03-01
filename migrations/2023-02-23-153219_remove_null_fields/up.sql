-- Your SQL goes here

ALTER TABLE nodes ALTER COLUMN blockchain_id SET NOT NULL;
ALTER TABLE nodes ALTER COLUMN name SET NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_by_user SET NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_for_org SET NOT NULL;
ALTER TABLE invitations ALTER COLUMN created_at SET NOT NULL;
ALTER TABLE orgs_users ALTER COLUMN role SET NOT NULL;
