CREATE TABLE node_reports (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    resource enum_api_resource NOT NULL,
    node_id UUID NOT NULL REFERENCES nodes ON DELETE CASCADE,
    created_by_user_id UUID NULL REFERENCES users ON DELETE SET NULL,
    created_by_node_id UUID NULL REFERENCES nodes ON DELETE SET NULL,
    created_by_host_id UUID NULL REFERENCES hosts ON DELETE SET NULL,
    created_by_org_id UUID NULL REFERENCES orgs ON DELETE SET NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
