ALTER TYPE enum_node_log_event ADD VALUE IF NOT EXISTS 'transferred_to_org';
ALTER TABLE node_logs ADD COLUMN org_id UUID NULL REFERENCES orgs;
UPDATE node_logs SET org_id = (SELECT nodes.org_id FROM nodes WHERE nodes.id = node_logs.node_id);
ALTER TABLE node_logs ALTER COLUMN org_id SET NOT NULL;

ALTER TABLE node_logs ADD COLUMN blockchain_id UUID NULL REFERENCES blockchains;
UPDATE node_logs SET blockchain_id = (SELECT blockchains.id FROM blockchains WHERE LOWER(blockchains.name) = LOWER(blockchain_name));
ALTER TABLE node_logs ALTER COLUMN blockchain_id SET NOT NULL;
ALTER TABLE node_logs DROP COLUMN blockchain_name;
