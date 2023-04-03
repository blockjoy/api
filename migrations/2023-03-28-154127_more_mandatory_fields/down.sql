-- This file should undo anything in `up.sql`

ALTER TABLE hosts ALTER COLUMN ip_range_from DROP NOT NULL;
ALTER TABLE hosts ALTER COLUMN ip_range_to DROP NOT NULL;
ALTER TABLE hosts ALTER COLUMN ip_gateway DROP NOT NULL;

UPDATE hosts SET ip_range_from = NULL WHERE ip_range_from = '127.0.0.1';
UPDATE hosts SET ip_range_to = NULL WHERE ip_range_to = '127.0.0.1';
UPDATE hosts SET ip_gateway = NULL WHERE ip_gateway = '127.0.0.1';


ALTER TABLE nodes ALTER COLUMN ip_addr DROP NOT NULL;

UPDATE nodes SET ip_addr = NULL WHERE ip_addr = '127.0.0.1';
