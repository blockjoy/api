-- This file should undo anything in `up.sql`

ALTER TABLE orgs_users DROP COLUMN host_provision_token;

CREATE TABLE host_provisions (
    id text PRIMARY KEY,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    claimed_at timestamp with time zone,
    host_id uuid,
    nodes text,
    ip_range_from inet,
    ip_range_to inet,
    ip_gateway inet                                    
);
