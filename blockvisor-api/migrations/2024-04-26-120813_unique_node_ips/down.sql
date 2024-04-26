ALTER TABLE nodes ADD COLUMN ip_addr TEXT NULL;
UPDATE nodes SET ip_addr = abbrev(ip);
ALTER TABLE nodes ALTER COLUMN ip_addr SET NOT NULL;
ALTER TABLE nodes DROP COLUMN ip;
DROP INDEX idx_blockchains_name_gin;
