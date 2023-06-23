ALTER TYPE enum_container_status RENAME TO enum_container_status_old;
CREATE TYPE enum_container_status AS ENUM (
    'unknown',
    'creating',
    'running',
    'starting',
    'stopping',
    'stopped',
    'upgrading',
    'upgraded',
    'deleting',
    'deleted',
    'installing',
    'snapshotting'
);
ALTER TABLE nodes ALTER COLUMN container_status DROP DEFAULT;
ALTER TABLE nodes ALTER COLUMN container_status TYPE enum_container_status USING container_status::text::enum_container_status;
ALTER TABLE nodes ALTER COLUMN container_status SET DEFAULT 'unknown';
DROP TYPE enum_container_status_old;
