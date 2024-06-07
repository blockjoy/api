ALTER TABLE orgs ADD COLUMN stripe_customer_id TEXT NULL;
ALTER TABLE users DROP COLUMN stripe_customer_id;
