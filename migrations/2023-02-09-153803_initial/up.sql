DROP TABLE IF EXISTS _sqlx_migrations;


-- For our next trick, if the migrations up to here have already been ran by SQLx, then the type
-- 'enum_blockchain_status' will already exist. Therefore we only perform the initial migration
DO $$
BEGIN
IF NOT EXISTS (SELECT TRUE FROM pg_type WHERE typname = 'enum_blockchain_status') THEN

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


CREATE TYPE enum_blockchain_status AS ENUM (
    'development',
    'alpha',
    'beta',
    'production',
    'deleted'
);

CREATE TYPE enum_conn_status AS ENUM (
    'online',
    'offline'
);

CREATE TYPE enum_container_status AS ENUM (
    'unknown',
    'creating',
    'running',
    'starting',
    'stopping',
    'stopped',
    'upgrading',
    'upgraded',
    'deleting',
    'deleted',
    'installing',
    'snapshotting'
);

CREATE TYPE enum_host_cmd AS ENUM (
    'create_node',
    'restart_node',
    'kill_node',
    'shutdown_node',
    'delete_node',
    'update_node',
    'migrate_node',
    'get_node_version',
    'get_bvs_version',
    'update_bvs',
    'restart_bvs',
    'remove_bvs',
    'stop_bvs',
    'create_bvs'
);

CREATE TYPE enum_host_type AS ENUM (
    'cloud',
    'enterprise'
);

CREATE TYPE enum_node_chain_status AS ENUM (
    'unknown',
    'broadcasting',
    'cancelled',
    'delegating',
    'delinquent',
    'disabled',
    'earning',
    'electing',
    'elected',
    'exported',
    'ingesting',
    'mining',
    'minting',
    'processing',
    'relaying',
    'removed',
    'removing',
    'provisioning'
);

CREATE TYPE enum_node_staking_status AS ENUM (
    'unknown',
    'follower',
    'staked',
    'staking',
    'validating',
    'consensus',
    'unstaked'
);

CREATE TYPE enum_node_sync_status AS ENUM (
    'unknown',
    'syncing',
    'synced'
);

CREATE TYPE enum_node_type AS ENUM (
    'undefined',
    'api',
    'etl',
    'node',
    'oracle',
    'relay',
    'validator',
    'miner'
);

CREATE TYPE enum_org_role AS ENUM (
    'admin',
    'owner',
    'member'
);

CREATE TYPE enum_stake_status AS ENUM (
    'available',
    'staking',
    'staked',
    'delinquent',
    'disabled'
);

CREATE TYPE enum_token_role AS ENUM (
    'user',
    'admin',
    'service',
    'guest',
    'pwd_reset'
);

CREATE TYPE enum_validator_status AS ENUM (
    'provisioning',
    'syncing',
    'upgrading',
    'migrating',
    'synced',
    'consensus',
    'stopped'
);

CREATE TYPE token_type AS ENUM (
    'login',
    'refresh',
    'pwd_reset',
    'user_auth',
    'host_auth',
    'user_refresh',
    'host_refresh'
);

CREATE TABLE IF NOT EXISTS blockchains (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name text NOT NULL,
    description text,
    status enum_blockchain_status DEFAULT 'development'::enum_blockchain_status NOT NULL,
    project_url text,
    repo_url text,
    supports_etl boolean DEFAULT true NOT NULL,
    supports_node boolean DEFAULT true NOT NULL,
    supports_staking boolean DEFAULT true NOT NULL,
    supports_broadcast boolean DEFAULT true NOT NULL,
    version text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    token text,
    supported_node_types jsonb DEFAULT '[]'::jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS broadcast_filters (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    blockchain_id uuid NOT NULL,
    org_id uuid NOT NULL,
    name text NOT NULL,
    callback_url text NOT NULL,
    auth_token text NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    last_processed_height bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    addresses jsonb,
    txn_types jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS broadcast_logs (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    blockchain_id uuid NOT NULL,
    org_id uuid NOT NULL,
    broadcast_filter_id uuid NOT NULL,
    address_count bigint DEFAULT 0 NOT NULL,
    txn_count bigint DEFAULT 0 NOT NULL,
    event_type text NOT NULL,
    event_msg text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE IF NOT EXISTS commands (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    host_id uuid NOT NULL,
    cmd enum_host_cmd NOT NULL,
    sub_cmd text,
    response text,
    exit_status integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    resource_id uuid DEFAULT uuid_generate_v4() NOT NULL
);

CREATE TABLE IF NOT EXISTS host_provisions (
    id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    claimed_at timestamp with time zone,
    host_id uuid,
    nodes text,
    ip_range_from inet,
    ip_range_to inet,
    ip_gateway inet
);

CREATE TABLE IF NOT EXISTS hosts (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    version text,
    name text NOT NULL,
    location text,
    ip_addr text NOT NULL,
    status enum_conn_status DEFAULT 'offline'::enum_conn_status NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    cpu_count bigint,
    mem_size bigint,
    disk_size bigint,
    os text,
    os_version text,
    ip_range_from inet,
    ip_range_to inet,
    ip_gateway inet,
    used_cpu integer,
    used_memory bigint,
    used_disk_space bigint,
    load_one double precision,
    load_five double precision,
    load_fifteen double precision,
    network_received bigint,
    network_sent bigint,
    uptime bigint,
    host_type enum_host_type DEFAULT 'cloud'::enum_host_type
);

CREATE TABLE IF NOT EXISTS info (
    block_height bigint DEFAULT 0 NOT NULL,
    staked_count bigint DEFAULT 0,
    oracle_price bigint DEFAULT 0,
    total_rewards bigint DEFAULT 0
);

CREATE TABLE IF NOT EXISTS invitations (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    created_by_user uuid,
    created_for_org uuid,
    invitee_email text NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    accepted_at timestamp with time zone,
    declined_at timestamp with time zone,
    created_by_user_name text DEFAULT ''::text NOT NULL,
    created_for_org_name text DEFAULT ''::text NOT NULL
);

CREATE TABLE IF NOT EXISTS invoices (
    id integer NOT NULL,
    user_id uuid NOT NULL,
    amount bigint NOT NULL,
    validators_count bigint NOT NULL,
    starts_at timestamp with time zone NOT NULL,
    ends_at timestamp with time zone NOT NULL,
    is_paid boolean DEFAULT false NOT NULL,
    earnings bigint DEFAULT 0 NOT NULL,
    fee_bps bigint NOT NULL
);

CREATE SEQUENCE invoices_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

CREATE TABLE IF NOT EXISTS ip_addresses (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    ip inet NOT NULL,
    host_id uuid,
    is_assigned boolean DEFAULT false NOT NULL
);

CREATE TABLE IF NOT EXISTS node_key_files (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name text NOT NULL,
    content text NOT NULL,
    node_id uuid NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id uuid NOT NULL,
    host_id uuid NOT NULL,
    name text,
    groups text,
    version text,
    ip_addr text,
    address text,
    wallet_address text,
    block_height bigint,
    node_data jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    blockchain_id uuid,
    sync_status enum_node_sync_status DEFAULT 'unknown'::enum_node_sync_status NOT NULL,
    chain_status enum_node_chain_status DEFAULT 'provisioning'::enum_node_chain_status NOT NULL,
    staking_status enum_node_staking_status DEFAULT 'unknown'::enum_node_staking_status,
    container_status enum_container_status DEFAULT 'unknown'::enum_container_status NOT NULL,
    node_type jsonb DEFAULT '{}'::jsonb NOT NULL,
    ip_gateway text DEFAULT ''::text NOT NULL,
    self_update boolean DEFAULT false NOT NULL,
    block_age bigint,
    consensus boolean,
    vcpu_count bigint DEFAULT 0 NOT NULL,
    mem_size_mb bigint DEFAULT 0 NOT NULL,
    disk_size_gb bigint DEFAULT 0 NOT NULL,
    host_name text DEFAULT ''::text NOT NULL,
    network text DEFAULT ''::text NOT NULL
);

CREATE TABLE IF NOT EXISTS orgs (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name text NOT NULL,
    is_personal boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone
);

CREATE TABLE IF NOT EXISTS orgs_users (
    org_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role enum_org_role,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE IF NOT EXISTS payments (
    hash text NOT NULL,
    user_id uuid NOT NULL,
    block bigint NOT NULL,
    payer text NOT NULL,
    payee text NOT NULL,
    amount bigint NOT NULL,
    oracle_price bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);

CREATE TABLE IF NOT EXISTS rewards (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    block bigint NOT NULL,
    hash text NOT NULL,
    validator_id uuid NOT NULL,
    account text NOT NULL,
    amount bigint NOT NULL,
    txn_time timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    user_id uuid,
    validator text NOT NULL
);

CREATE TABLE IF NOT EXISTS token_blacklist (
    token text NOT NULL,
    token_type token_type NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    email text NOT NULL,
    hashword text NOT NULL,
    salt text NOT NULL,
    refresh text DEFAULT uuid_generate_v4(),
    fee_bps bigint DEFAULT 500 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    staking_quota bigint DEFAULT 3 NOT NULL,
    pay_address text,
    first_name character varying(64) NOT NULL,
    last_name character varying(64) NOT NULL,
    confirmed_at timestamp with time zone,
    deleted_at timestamp with time zone
);

ALTER TABLE ONLY invoices ALTER COLUMN id SET DEFAULT nextval('invoices_id_seq'::regclass);

ALTER TABLE ONLY rewards
    ADD CONSTRAINT const_rewards_val_hash_u UNIQUE (validator, hash);

ALTER TABLE ONLY info
    ADD CONSTRAINT info_pkey PRIMARY KEY (block_height);

ALTER TABLE ONLY ip_addresses
    ADD CONSTRAINT ip_addresses_ip_key UNIQUE (ip);

ALTER TABLE ONLY node_key_files
    ADD CONSTRAINT node_key_files_name_node_id_key UNIQUE (name, node_id);

ALTER TABLE ONLY orgs_users
    ADD CONSTRAINT orgs_users_pkey PRIMARY KEY (org_id, user_id);

ALTER TABLE ONLY payments
    ADD CONSTRAINT payments_pkey PRIMARY KEY (hash);

ALTER TABLE ONLY token_blacklist
    ADD CONSTRAINT token_blacklist_pkey PRIMARY KEY (token);

ALTER TABLE ONLY users
    ADD CONSTRAINT users_email_key UNIQUE (email);

ALTER TABLE ONLY users
    ADD CONSTRAINT users_refresh_key UNIQUE (refresh);

CREATE UNIQUE INDEX idx_blockchains_name ON nodes USING btree (lower(name));

CREATE INDEX idx_broadcast_filters_blockchain_id ON broadcast_filters USING btree (blockchain_id);

CREATE INDEX idx_broadcast_filters_created_at ON broadcast_filters USING btree (created_at);

CREATE INDEX idx_broadcast_filters_is_active ON broadcast_filters USING btree (is_active);

CREATE INDEX idx_broadcast_filters_org_id ON broadcast_filters USING btree (org_id);

CREATE UNIQUE INDEX idx_broadcast_filters_org_id_name ON nodes USING btree (org_id, lower(name));

CREATE INDEX idx_broadcast_filters_updated_at ON broadcast_filters USING btree (updated_at);

CREATE INDEX idx_broadcast_logs_blockchain_id ON broadcast_logs USING btree (blockchain_id);

CREATE INDEX idx_broadcast_logs_broadcast_filter_id ON broadcast_logs USING btree (broadcast_filter_id);

CREATE INDEX idx_broadcast_logs_created_at ON broadcast_logs USING btree (created_at);

CREATE INDEX idx_broadcast_logs_event_type ON broadcast_logs USING btree (blockchain_id);

CREATE INDEX idx_broadcast_logs_org_id ON broadcast_logs USING btree (org_id);

CREATE INDEX idx_commands_completed_at ON commands USING btree (completed_at);

CREATE INDEX idx_commands_created_at ON commands USING btree (created_at);

CREATE INDEX idx_commands_host_id ON commands USING btree (host_id);

CREATE INDEX idx_host_provisions_claimed_at ON host_provisions USING btree (claimed_at);

CREATE INDEX idx_host_provisions_created_at ON host_provisions USING btree (created_at);

CREATE INDEX idx_host_provisions_host_id ON host_provisions USING btree (host_id);

CREATE INDEX idx_invoices_ends_at ON invoices USING btree (ends_at);

CREATE INDEX idx_invoices_is_paid ON invoices USING btree (is_paid);

CREATE INDEX idx_invoices_starts_at ON invoices USING btree (starts_at);

CREATE INDEX idx_invoices_user_id ON invoices USING btree (user_id);

CREATE INDEX idx_ip_addrs ON ip_addresses USING btree (ip);

CREATE INDEX idx_nodes_created_at ON nodes USING btree (created_at);

CREATE INDEX idx_nodes_groups ON nodes USING btree (lower(groups));

CREATE INDEX idx_nodes_host_id ON nodes USING btree (host_id);

CREATE INDEX idx_nodes_node_data ON nodes USING btree (node_data);

CREATE INDEX idx_nodes_org_id ON nodes USING btree (org_id);

CREATE UNIQUE INDEX idx_nodes_org_id_name ON nodes USING btree (org_id, lower(name));

CREATE INDEX idx_orgs_is_personal ON orgs USING btree (is_personal);

CREATE INDEX idx_payments_payer ON payments USING btree (payer);

CREATE INDEX idx_payments_user_id ON payments USING btree (user_id);

CREATE INDEX idx_paymets_payee ON payments USING btree (payee);

CREATE INDEX idx_rewards_account ON rewards USING btree (account);

CREATE INDEX idx_rewards_block ON rewards USING btree (block);

CREATE INDEX idx_rewards_created_at ON rewards USING btree (created_at);

CREATE INDEX idx_rewards_txn_time ON rewards USING btree (txn_time);

CREATE INDEX idx_rewards_user_id ON rewards USING btree (user_id);

CREATE INDEX idx_rewards_validator_id ON rewards USING btree (validator_id);

CREATE UNIQUE INDEX idx_users_email ON users USING btree (email);

CREATE UNIQUE INDEX idx_users_refresh ON users USING btree (refresh);

ALTER TABLE ONLY broadcast_filters
    ADD CONSTRAINT broadcast_filters_blockchain_id_fkey FOREIGN KEY (blockchain_id) REFERENCES blockchains(id) ON DELETE CASCADE;

ALTER TABLE ONLY broadcast_filters
    ADD CONSTRAINT broadcast_filters_org_id_fkey FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE;

ALTER TABLE ONLY commands
    ADD CONSTRAINT fk_host_commands_hosts FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE;

ALTER TABLE ONLY invoices
    ADD CONSTRAINT fk_invoices_users FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY payments
    ADD CONSTRAINT fk_payments_users FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE CASCADE ON DELETE SET NULL;

ALTER TABLE ONLY host_provisions
    ADD CONSTRAINT host_provisions_host_id_fkey FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE;

ALTER TABLE ONLY invitations
    ADD CONSTRAINT invitations_created_by_user_fkey FOREIGN KEY (created_by_user) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY invitations
    ADD CONSTRAINT invitations_created_for_org_fkey FOREIGN KEY (created_for_org) REFERENCES orgs(id) ON DELETE CASCADE;

ALTER TABLE ONLY ip_addresses
    ADD CONSTRAINT ip_addresses_host_id_fkey FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE;

ALTER TABLE ONLY node_key_files
    ADD CONSTRAINT node_key_files_node_id_fkey FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE;

ALTER TABLE ONLY nodes
    ADD CONSTRAINT nodes_blockchain_id_fkey FOREIGN KEY (blockchain_id) REFERENCES blockchains(id) ON DELETE CASCADE;

ALTER TABLE ONLY nodes
    ADD CONSTRAINT nodes_host_id_fkey FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE;

ALTER TABLE ONLY nodes
    ADD CONSTRAINT nodes_org_id_fkey FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE;

ALTER TABLE ONLY orgs_users
    ADD CONSTRAINT orgs_users_org_id_fkey FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE;

ALTER TABLE ONLY orgs_users
    ADD CONSTRAINT orgs_users_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

END IF;
END$$;
