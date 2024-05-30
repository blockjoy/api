ALTER TABLE users RENAME COLUMN billing_id TO chargebee_billing_id;
ALTER TABLE users ADD COLUMN stripe_customer_id TEXT NULL;
