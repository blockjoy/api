ALTER TABLE users
    ADD COLUMN IF NOT EXISTS deleted_at timestamp with time zone default null;
