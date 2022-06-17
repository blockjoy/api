CREATE TYPE enum_groupable_type AS ENUM ('host', 'node');

CREATE TABLE IF NOT EXISTS groups (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name Text NOT NULL,
    org_id UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_groups_org_id on groups(org_id, lower(name));

CREATE TABLE IF NOT EXISTS groupable (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    groupable_id UUID NOT NULL,
    groupable_type enum_groupable_type NOT NULL
);
