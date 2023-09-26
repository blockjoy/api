create type enum_org_role as enum (
    'admin',
    'owner',
    'member'
);

alter table orgs_users add column role enum_org_role;
alter table users add column is_blockjoy_admin boolean not null default false;

update orgs_users as ou
set role = q.role
from (
    select user_id, org_id, case role
        when 'org-admin' then 'admin'::enum_org_role
        when 'org-owner' then 'owner'::enum_org_role
        when 'org-member' then 'member'::enum_org_role
        else null
    end as role
    from user_roles
) as q
where ou.user_id = q.user_id and ou.org_id = q.org_id;

drop table if exists user_roles;
drop table if exists role_permissions;
drop table if exists roles;
drop table if exists permissions;

