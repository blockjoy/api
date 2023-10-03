ALTER TABLE nodes DROP COLUMN jobs JSONB NULL;

ALTER TABLE nodes ADD COLUMN data_sync_progress_total INT NULL;
ALTER TABLE nodes ADD COLUMN data_sync_progress_current INT NULL;
ALTER TABLE nodes ADD COLUMN data_sync_progress_message TEXT NULL;
