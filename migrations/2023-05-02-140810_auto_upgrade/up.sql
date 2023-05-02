-- Your SQL goes here
ALTER TABLE nodes ADD COLUMN self_upgrade JSONB DEFAULT '{"enabled": false, "policy": "upgrade_all"}',
                  ALTER COLUMN self_upgrade SET NOT NULL;
UPDATE nodes SET self_upgrade = jsonb_build_object('enabled',self_update, 'policy', 'upgrade_all');

ALTER TABLE nodes DROP COLUMN self_update;

