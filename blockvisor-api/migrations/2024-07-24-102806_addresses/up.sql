CREATE TABLE addresses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    city VARCHAR(256) NULL,
    country VARCHAR(256) NULL,
    line1 VARCHAR(256) NULL,
    line2 VARCHAR(256) NULL,
    postal_code VARCHAR(256) NULL,
    state VARCHAR(256) NULL
);

ALTER TABLE orgs ADD COLUMN address_id UUID NULL REFERENCES addresses ON DELETE SET NULL;
