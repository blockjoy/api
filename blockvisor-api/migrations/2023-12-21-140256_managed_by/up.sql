CREATE TYPE enum_managed_by AS ENUM (
    'automatic',
    'manual'
);
ALTER TABLE hosts ADD COLUMN managed_by enum_managed_by NOT NULL DEFAULT 'automatic';
