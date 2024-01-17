create type enum_blockchain_visibility as enum (
  'development',
  'private',
  'public'
);

alter table blockchains
add column visibility enum_blockchain_visibility
not null default 'private'::enum_blockchain_visibility;

alter table blockchain_node_types
add column visibility enum_blockchain_visibility
not null default 'private'::enum_blockchain_visibility;
