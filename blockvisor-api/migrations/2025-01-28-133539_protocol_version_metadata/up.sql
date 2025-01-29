alter table protocol_versions
    add column metadata jsonb not null default '[]';
