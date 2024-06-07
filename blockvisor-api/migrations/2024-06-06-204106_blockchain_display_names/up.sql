ALTER TABLE blockchains ADD COLUMN display_name TEXT NULL;
UPDATE blockchains SET display_name = name;
ALTER TABLE blockchains ALTER COLUMN display_name SET NOT NULL;
