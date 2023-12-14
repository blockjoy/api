create table if not exists node_key_files (
    id uuid primary key default uuid_generate_v4(),
    node_id uuid not null,
    name text not null,
    content text not null
);
