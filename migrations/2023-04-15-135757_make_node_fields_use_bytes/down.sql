ALTER TABLE nodes RENAME COLUMN mem_size_bytes TO mem_size_mb;
ALTER TABLE nodes ALTER COLUMN mem_size_mb SET DEFAULT 0;
UPDATE nodes SET mem_size_mb = mem_size_mb / 1000 / 1000;

ALTER TABLE nodes RENAME COLUMN disk_size_bytes TO disk_size_gb;
ALTER TABLE nodes ALTER COLUMN disk_size_gb SET DEFAULT 0;
UPDATE nodes SET disk_size_gb = disk_size_gb / 1000 / 1000 / 1000;

ALTER TABLE nodes ALTER COLUMN vcpu_count SET DEFAULT 0;


ALTER TABLE hosts RENAME COLUMN mem_size_bytes TO mem_size;
ALTER TABLE hosts RENAME COLUMN disk_size_bytes TO disk_size;
