ALTER TABLE node_logs DROP COLUMN org_id;

ALTER TABLE node_logs ADD COLUMN blockchain_name TEXT NULL;
UPDATE node_logs SET blockchain_name = (SELECT blockchains.name FROM blockchains WHERE blockchains.id = blockchain_id);
ALTER TABLE node_logs ALTER COLUMN blockchain_name SET NOT NULL;
ALTER TABLE node_logs DROP COLUMN blockchain_id;
