UPDATE nodes SET dns_record_id = '' WHERE dns_record_id IS NULL;
ALTER TABLE nodes ALTER COLUMN dns_record_id SET NOT NULL;
ALTER TABLE nodes ALTER COLUMN dns_record_id ADD DEFAULT '';
