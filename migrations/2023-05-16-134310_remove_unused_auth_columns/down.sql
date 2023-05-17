-- This file should undo anything in `up.sql`

ALTER TABLE users ADD COLUMN refresh text DEFAULT uuid_generate_v4();
ALTER TABLE users ADD COLUMN staking_quota bigint DEFAULT 3 NOT NULL;
ALTER TABLE host_provisions ADD COLUMN nodes text;
