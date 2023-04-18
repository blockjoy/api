ALTER TABLE nodes RENAME COLUMN mem_size_mb TO mem_size_bytes;
ALTER TABLE nodes ALTER COLUMN mem_size_bytes DROP DEFAULT;
UPDATE nodes SET mem_size_bytes = mem_size_bytes * 1000 * 1000;

ALTER TABLE nodes RENAME COLUMN disk_size_gb TO disk_size_bytes;
ALTER TABLE nodes ALTER COLUMN disk_size_bytes DROP DEFAULT;
UPDATE nodes SET disk_size_bytes = disk_size_bytes * 1000 * 1000 * 1000;

ALTER TABLE nodes ALTER COLUMN vcpu_count DROP DEFAULT;


ALTER TABLE hosts RENAME COLUMN mem_size TO mem_size_bytes;
ALTER TABLE hosts RENAME COLUMN disk_size TO disk_size_bytes;
