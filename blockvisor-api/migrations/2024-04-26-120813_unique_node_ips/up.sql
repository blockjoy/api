ALTER TABLE nodes ADD COLUMN ip INET NULL;
UPDATE nodes SET ip = ip_addr::INET;
ALTER TABLE nodes ALTER COLUMN ip SET NOT NULL;
ALTER TABLE nodes DROP COLUMN ip_addr;
CREATE UNIQUE INDEX idx_nodes_ip ON nodes (ip) WHERE deleted_at IS NULL;
CREATE INDEX idx_blockchains_name_gin ON nodes USING GIN (to_tsvector('english', lower(name)));
CREATE INDEX idx_blockchains_ip_gin ON nodes USING GIN (to_tsvector('english', abbrev(ip)));
