create table if not exists roles (
    name text primary key,
    created_at timestamp with time zone default now() not null
);

create table if not exists permissions (
    name text primary key,
    created_at timestamp with time zone default now() not null
);

create table if not exists role_permissions (
    role text references roles(name) on delete cascade,
    permission text references permissions(name) on delete cascade,
    created_at timestamp with time zone default now() not null,
    primary key (role, permission)
);

create table if not exists user_roles (
    user_id uuid references users(id) on delete cascade,
    org_id uuid references orgs(id) on delete cascade,
    role text references roles(name) on delete cascade,
    created_at timestamp with time zone default now() not null,
    primary key (user_id, org_id, role)
);

create index idx_user_roles_role on user_roles using btree (role);

insert into user_roles (user_id, org_id, role)
select
    user_id,
    org_id,
    case role
        when 'admin' then 'org-admin'
        when 'owner' then 'org-owner'
        when 'member' then 'org-member'
        else null
    end
from orgs_users;

alter table users drop column is_blockjoy_admin;
alter table orgs_users drop column role;

drop type enum_org_role;
