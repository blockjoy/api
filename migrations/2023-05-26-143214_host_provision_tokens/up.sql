-- Your SQL goes here

DROP TABLE host_provisions;

ALTER TABLE orgs_users ADD COLUMN host_provision_token VARCHAR(32) NULL;
UPDATE orgs_users SET host_provision_token = substr(md5(random()::text), 0, 12);
ALTER TABLE orgs_users ALTER COLUMN host_provision_token SET NOT NULL;

ALTER TABLE hosts ADD COLUMN created_by UUID NULL REFERENCES users ON DELETE RESTRICT;
