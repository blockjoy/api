CREATE TABLE IF NOT EXISTS subscriptions (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id uuid UNIQUE NOT NULL,
    user_id uuid NOT NULL,
    external_id text NOT NULL,
    CONSTRAINT fk_org_id FOREIGN KEY(org_id) REFERENCES orgs(id) ON DELETE CASCADE,
    CONSTRAINT fk_user_id FOREIGN KEY(user_id) REFERENCES users(id)
);

CREATE INDEX idx_subs_user_id ON subscriptions USING btree (user_id);
