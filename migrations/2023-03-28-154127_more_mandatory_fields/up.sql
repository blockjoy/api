-- Your SQL goes here

UPDATE hosts SET ip_range_from = '127.0.0.1' WHERE ip_range_from IS NULL;
UPDATE hosts SET ip_range_to = '127.0.0.1' WHERE ip_range_to IS NULL;
UPDATE hosts SET ip_gateway = '127.0.0.1' WHERE ip_gateway IS NULL;

ALTER TABLE hosts ALTER COLUMN ip_range_from SET NOT NULL;
ALTER TABLE hosts ALTER COLUMN ip_range_to SET NOT NULL;
ALTER TABLE hosts ALTER COLUMN ip_gateway SET NOT NULL;


UPDATE nodes SET ip_addr = '127.0.0.1' WHERE ip_addr IS NULL;

ALTER TABLE nodes ALTER COLUMN ip_addr SET NOT NULL;
