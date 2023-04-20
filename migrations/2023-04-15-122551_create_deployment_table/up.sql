-- Your SQL goes here

CREATE TYPE enum_node_log_event AS ENUM (
    'created',
    'succeeded',
    'failed',
    'canceled'
);

-- Note, this table serves as a log for attempts at the deployment of a node. For this reason, it is
-- to be treated as append-only. We write to this table when an ack-message comes back from
-- blockvisord, detailing the status of the node deployment.
CREATE TABLE node_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    -- We don't put foreign keys in here, because we want to keep the UUID's around when the hosts
    -- or nodes get deleted.
    host_id UUID NOT NULL,
    node_id UUID NOT NULL,
    event enum_node_log_event NOT NULL,
    blockchain_name TEXT NOT NULL,
    node_type enum_node_type NOT NULL,
    version VARCHAR(32) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX node_logs_host_id_idx ON node_logs USING btree (host_id);
CREATE INDEX node_logs_node_id_idx ON node_logs USING btree (node_id);
CREATE INDEX node_logs_created_at_idx ON node_logs USING btree (created_at);
CREATE INDEX node_logs_event_idx ON node_logs USING btree (event);
CREATE INDEX node_logs_blockchain_name_idx ON node_logs USING btree (blockchain_name);
