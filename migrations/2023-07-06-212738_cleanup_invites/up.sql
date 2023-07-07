ALTER TABLE invitations RENAME COLUMN created_by_user TO created_by;
ALTER TABLE invitations RENAME COLUMN created_for_org TO org_id;
ALTER TABLE invitations DROP COLUMN created_by_user_name;
ALTER TABLE invitations DROP COLUMN created_for_org_name;
