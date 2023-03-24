ALTER TABLE nodes ALTER COLUMN dns_record_id DROP NOT NULL;
ALTER TABLE nodes ALTER COLUMN dns_record_id DROP DEFAULT;
UPDATE nodes SET dns_record_id = NULL WHERE dns_record_id = '';
