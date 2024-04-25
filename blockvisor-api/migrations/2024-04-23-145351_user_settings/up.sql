CREATE TABLE user_settings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users ON DELETE CASCADE,
    name TEXT NOT NULL,
    value BYTEA NOT NULL,
    UNIQUE (user_id, name)
);
