CREATE TABLE regions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL UNIQUE
);

ALTER TABLE hosts ADD COLUMN region_id UUID NULL REFERENCES regions ON DELETE SET NULL;
ALTER TABLE nodes ADD COLUMN scheduler_region UUID NULL REFERENCES regions ON DELETE SET NULL;

ALTER TABLE hosts ALTER COLUMN host_type TYPE TEXT USING host_type::TEXT;
ALTER TABLE hosts ALTER COLUMN host_type DROP DEFAULT;
UPDATE hosts SET host_type = 'private' WHERE host_type = 'enterprise';
DROP TYPE enum_host_type;
CREATE TYPE enum_host_type AS ENUM (
    'cloud',
    'private'
);
ALTER TABLE hosts ALTER COLUMN host_type TYPE enum_host_type USING host_type::enum_host_type;
