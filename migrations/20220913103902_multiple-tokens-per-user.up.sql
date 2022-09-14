-- This enum will indicate which kind of token we are dealing with here.
CREATE TYPE token_type AS ENUM ('login', 'refresh', 'pwd_reset');

-- We add the column token_type of the tokens table, give it a one-off default of `'login'` and make
-- it required.
ALTER TABLE tokens ADD COLUMN type token_type NULL;
UPDATE tokens SET type = 'login';
ALTER TABLE tokens ALTER COLUMN type SET NOT NULL;

-- This is a linking table between users and tokens, and it indicates who which tokens belong to.
-- The primary key is composed of the user id and the token type, meaning that each user can have
-- only one linked token of each type. 
CREATE TABLE user_tokens (
    user_id UUID NOT NULL REFERENCES users ON DELETE CASCADE,
    token_id UUID NOT NULL REFERENCES tokens ON DELETE CASCADE,
    token_type token_type NOT NULL,
    PRIMARY KEY (user_id, token_type)
);

-- For each user, we take their currently configured token_id and copy it over into the new
-- `user_tokens` table.
INSERT INTO user_tokens (SELECT id, token_id, 'login' FROM users WHERE token_id IS NOT NULL);

-- Now that the data is in user_tokens, we can drop the token_id column from the users table.
ALTER TABLE users DROP COLUMN token_id;


-- Next up we do pretty much the same thing for hosts
CREATE TABLE host_tokens (
    host_id UUID NOT NULL REFERENCES hosts ON DELETE CASCADE,
    token_id UUID NOT NULL REFERENCES tokens ON DELETE CASCADE,
    token_type token_type NOT NULL,
    PRIMARY KEY (host_id, token_type)
);
INSERT INTO host_tokens (SELECT id, token_id, 'login' FROM hosts WHERE token_id IS NOT NULL);
ALTER TABLE hosts DROP COLUMN token_id;

ALTER TYPE enum_token_role ADD VALUE IF NOT EXISTS 'pwd_reset';
