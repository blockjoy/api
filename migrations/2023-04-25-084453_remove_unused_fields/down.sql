ALTER TABLE users ADD COLUMN pay_address TEXT;
ALTER TABLE users ADD COLUMN fee_bps BIGINT DEFAULT 500,
                  ALTER COLUMN fee_bps SET NOT NULL;


ALTER TABLE blockchains ADD COLUMN token TEXT;
ALTER TABLE blockchains ADD COLUMN supports_etl BOOLEAN DEFAULT TRUE,
                         ALTER COLUMN supports_etl SET NOT NULL;
ALTER TABLE blockchains ADD COLUMN supports_node BOOLEAN DEFAULT TRUE,
                         ALTER COLUMN supports_node SET NOT NULL;
ALTER TABLE blockchains ADD COLUMN supports_staking BOOLEAN DEFAULT TRUE,
                         ALTER COLUMN supports_staking SET NOT NULL;
ALTER TABLE blockchains ADD COLUMN supports_broadcast BOOLEAN DEFAULT TRUE,
                         ALTER COLUMN supports_broadcast SET NOT NULL;


-- Recreate payments table with relations, indexes and constraints
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
ALTER TABLE ONLY payments
    ADD CONSTRAINT payments_pkey PRIMARY KEY (hash);

CREATE INDEX idx_payments_payer ON payments USING btree (payer);

CREATE INDEX idx_payments_user_id ON payments USING btree (user_id);

CREATE INDEX idx_paymets_payee ON payments USING btree (payee);

ALTER TABLE ONLY payments
    ADD CONSTRAINT fk_payments_users FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE CASCADE ON DELETE SET NULL;


-- Recreate invoices table with relations, indexes and constraints
CREATE TABLE IF NOT EXISTS invoices (
    id integer PRIMARY KEY,
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

ALTER TABLE ONLY invoices ALTER COLUMN id SET DEFAULT nextval('invoices_id_seq'::regclass);

CREATE INDEX idx_invoices_ends_at ON invoices USING btree (ends_at);

CREATE INDEX idx_invoices_is_paid ON invoices USING btree (is_paid);

CREATE INDEX idx_invoices_starts_at ON invoices USING btree (starts_at);

CREATE INDEX idx_invoices_user_id ON invoices USING btree (user_id);

ALTER TABLE ONLY invoices
    ADD CONSTRAINT fk_invoices_users FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

-- Recreate rewards table with relations, indexes and constraints
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

ALTER TABLE ONLY rewards
    ADD CONSTRAINT const_rewards_val_hash_u UNIQUE (validator, hash);

CREATE INDEX idx_rewards_account ON rewards USING btree (account);

CREATE INDEX idx_rewards_block ON rewards USING btree (block);

CREATE INDEX idx_rewards_created_at ON rewards USING btree (created_at);

CREATE INDEX idx_rewards_txn_time ON rewards USING btree (txn_time);

CREATE INDEX idx_rewards_user_id ON rewards USING btree (user_id);

CREATE INDEX idx_rewards_validator_id ON rewards USING btree (validator_id);

-- Recreate broadcast_filters table with relations, indexes and constraints
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

CREATE INDEX idx_broadcast_filters_blockchain_id ON broadcast_filters USING btree (blockchain_id);

CREATE INDEX idx_broadcast_filters_created_at ON broadcast_filters USING btree (created_at);

CREATE INDEX idx_broadcast_filters_is_active ON broadcast_filters USING btree (is_active);

CREATE INDEX idx_broadcast_filters_org_id ON broadcast_filters USING btree (org_id);

CREATE UNIQUE INDEX idx_broadcast_filters_org_id_name ON nodes USING btree (org_id, lower(name));

CREATE INDEX idx_broadcast_filters_updated_at ON broadcast_filters USING btree (updated_at);

ALTER TABLE ONLY broadcast_filters
    ADD CONSTRAINT broadcast_filters_blockchain_id_fkey FOREIGN KEY (blockchain_id) REFERENCES blockchains(id) ON DELETE CASCADE;

ALTER TABLE ONLY broadcast_filters
    ADD CONSTRAINT broadcast_filters_org_id_fkey FOREIGN KEY (org_id) REFERENCES orgs(id) ON DELETE CASCADE;


-- Recreate broadcast_logs table with relations, indexes and constraints
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

CREATE INDEX idx_broadcast_logs_blockchain_id ON broadcast_logs USING btree (blockchain_id);

CREATE INDEX idx_broadcast_logs_broadcast_filter_id ON broadcast_logs USING btree (broadcast_filter_id);

CREATE INDEX idx_broadcast_logs_created_at ON broadcast_logs USING btree (created_at);

CREATE INDEX idx_broadcast_logs_event_type ON broadcast_logs USING btree (blockchain_id);

CREATE INDEX idx_broadcast_logs_org_id ON broadcast_logs USING btree (org_id);

-- Recreate info table with relations, indexes and constraints
CREATE TABLE IF NOT EXISTS info (
    block_height bigint DEFAULT 0 NOT NULL,
    staked_count bigint DEFAULT 0,
    oracle_price bigint DEFAULT 0,
    total_rewards bigint DEFAULT 0
);

ALTER TABLE ONLY info
    ADD CONSTRAINT info_pkey PRIMARY KEY (block_height);

-- Recreate nodes fields
ALTER TABLE nodes ADD COLUMN groups TEXT;
ALTER TABLE nodes ALTER COLUMN version SET NULL;

ALTER TABLE hosts ADD COLUMN location TEXT;
