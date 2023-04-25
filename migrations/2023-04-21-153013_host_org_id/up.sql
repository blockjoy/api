-- Your SQL goes here

-- The scheduler will be optional, so we have to allow passing NULL here.
ALTER TABLE nodes ALTER COLUMN scheduler_resource DROP NOT NULL;

ALTER TABLE hosts ADD COLUMN org_id UUID NULL REFERENCES orgs ON DELETE RESTRICT;

ALTER TABLE host_provisions ADD COLUMN org_id UUID NULL REFERENCES orgs ON DELETE CASCADE;
