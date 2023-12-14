CREATE TABLE node_reports (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    node_id UUID NOT NULL REFERENCES nodes ON DELETE CASCADE,
    created_by_resource enum_resource_type NOT NULL,
    created_by UUID NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
