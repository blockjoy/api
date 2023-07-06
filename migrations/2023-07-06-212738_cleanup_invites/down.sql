ALTER TABLE invitations RENAME COLUMN created_by TO created_by_user;
ALTER TABLE invitations RENAME COLUMN org_id TO created_for_org;
ALTER TABLE invitations ADD COLUMN created_by_user_name TEXT DEFAULT '';
ALTER TABLE invitations ADD COLUMN created_for_org_name TEXT DEFAULT '';
