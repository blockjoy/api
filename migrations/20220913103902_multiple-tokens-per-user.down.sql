ALTER TABLE users ADD COLUMN token_id UUID NULL REFERENCES tokens (id);
UPDATE
    users
SET
    token_id = (SELECT user_tokens.token_id FROM user_tokens WHERE user_id = users.id AND token_type = 'login');
DROP TABLE user_tokens;

ALTER TABLE hosts ADD COLUMN token_id UUID NULL REFERENCES tokens (id);
UPDATE
    hosts
SET
    token_id = (SELECT host_tokens.token_id FROM host_tokens WHERE host_id = hosts.id AND token_type = 'login');
DROP TABLE host_tokens;

ALTER TABLE tokens DROP COLUMN type;
DROP TYPE token_type;
