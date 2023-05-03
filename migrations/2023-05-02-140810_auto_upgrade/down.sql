-- This file should undo anything in `up.sql`
ALTER TABLE nodes DROP COLUMN self_upgrade;
ALTER TABLE nodes ADD COLUMN self_update boolean DEFAULT false,
                  ALTER COLUMN self_update SET NOT NULL;
