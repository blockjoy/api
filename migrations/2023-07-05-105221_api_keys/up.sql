CREATE TYPE enum_api_resource AS ENUM (
    'user',
    'org',
    'host',
    'node'
);

CREATE TABLE IF NOT EXISTS api_keys (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id uuid NOT NULL,
    label text NOT NULL,
    key_hash text NOT NULL,
    key_salt text NOT NULL,
    resource enum_api_resource NOT NULL,
    resource_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone,
    CONSTRAINT fk_user_id FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_api_keys_user_id ON api_keys USING btree (user_id);
