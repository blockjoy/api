ALTER TABLE users ADD COLUMN stripe_customer_id TEXT NULL;
ALTER TABLE orgs DROP COLUMN stripe_customer_id;
