ALTER TABLE users RENAME COLUMN chargebee_billing_id TO billing_id;
ALTER TABLE users DROP COLUMN stripe_customer_id;
