ALTER TABLE users ADD COLUMN first_name VARCHAR(64) NULL;
ALTER TABLE users ADD COLUMN last_name VARCHAR(64) NULL;

-- Temporary one-off default
UPDATE users SET first_name = 'Platzhalter';
UPDATE users SET last_name = 'Platzhalter';

ALTER TABLE users ALTER COLUMN first_name SET NOT NULL;
ALTER TABLE users ALTER COLUMN last_name SET NOT NULL;
