-- We have 24 bits of mac address space after fixing the first 24 bits. This
-- means that our mac addresses must not exceed 2^24 - 1, which is 16777215
CREATE SEQUENCE node_mac_addresses MINVALUE 0 MAXVALUE 16777215 NO CYCLE;
ALTER TABLE nodes ADD COLUMN mac_address macaddr NULL UNIQUE;
UPDATE nodes SET mac_address = TO_HEX(nextval('node_mac_addresses'))::macaddr;
ALTER TABLE nodes ALTER COLUMN mac_address SET NOT NULL;
