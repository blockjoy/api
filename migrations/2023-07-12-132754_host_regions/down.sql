ALTER TABLE hosts DROP COLUMN region_id;
ALTER TABLE nodes DROP COLUMN scheduler_region;
DROP TABLE regions;

ALTER TABLE hosts ALTER COLUMN host_type TYPE TEXT USING host_type::TEXT;
UPDATE hosts SET host_type = 'enterprise' WHERE host_type = 'private';
DROP TYPE enum_host_type;
CREATE TYPE enum_host_type AS ENUM (
    'cloud',
    'enterprise'
);
ALTER TABLE hosts ALTER COLUMN host_type TYPE enum_host_type USING host_type::enum_host_type;
ALTER TABLE hosts ALTER COLUMN host_type SET DEFAULT 'cloud'::enum_host_type;
