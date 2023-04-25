ALTER TABLE users DROP COLUMN pay_address;
ALTER TABLE users DROP COLUMN fee_bps;

ALTER TABLE blockchains DROP COLUMN token;
ALTER TABLE blockchains DROP COLUMN supports_etl;
ALTER TABLE blockchains DROP COLUMN supports_node;
ALTER TABLE blockchains DROP COLUMN supports_staking;
ALTER TABLE blockchains DROP COLUMN supports_broadcast;

DROP TABLE IF EXISTS payments;

DROP TABLE IF EXISTS invoices;
DROP SEQUENCE IF EXISTS invoices_id_seq;

DROP TABLE IF EXISTS rewards;

DROP TABLE IF EXISTS broadcast_filters;

DROP TABLE IF EXISTS broadcast_logs;

DROP TABLE IF EXISTS info;

ALTER TABLE nodes DROP COLUMN groups;
ALTER TABLE nodes ALTER COLUMN version SET NOT NULL;

ALTER TABLE hosts DROP COLUMN location;

DROP INDEX idx_broadcast_filters_org_id_name;
