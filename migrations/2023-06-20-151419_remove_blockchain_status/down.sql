CREATE TYPE enum_blockchain_status AS ENUM (
    'development',
    'alpha',
    'beta',
    'production',
    'deleted'
);
ALTER TABLE blockchains ADD COLUMN status enum_blockchain_status NULL;
UPDATE blockchains SET status = 'production';
ALTER TABLE blockchains ALTER COLUMN status SET NOT NULL;
