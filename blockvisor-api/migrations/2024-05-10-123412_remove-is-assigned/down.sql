ALTER TABLE ip_addresses ADD COLUMN is_assigned BOOL NULL;

UPDATE ip_addresses
SET is_assigned = TRUE
WHERE (
    EXISTS (SELECT id FROM nodes WHERE nodes.ip = ip_addresses.ip AND nodes.deleted_at IS NULL)
);

UPDATE ip_addresses
SET is_assigned = FALSE
WHERE (
    NOT EXISTS (SELECT id FROM nodes WHERE nodes.ip = ip_addresses.ip AND nodes.deleted_at IS NULL)
);

ALTER TABLE ip_addresses ALTER COLUMN is_assigned SET NOT NULL;
