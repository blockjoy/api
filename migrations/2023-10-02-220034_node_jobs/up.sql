ALTER TABLE nodes ADD COLUMN jobs JSONB NOT NULL DEFAULT '[]';

ALTER TABLE nodes DROP COLUMN data_sync_progress_total;
ALTER TABLE nodes DROP COLUMN data_sync_progress_current;
ALTER TABLE nodes DROP COLUMN data_sync_progress_message;
