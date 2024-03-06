ALTER TABLE nodes ADD COLUMN url TEXT NULL;
UPDATE nodes SET url = name || '.n0des.xyz';
ALTER TABLE nodes ALTER COLUMN url SET NOT NULL;
