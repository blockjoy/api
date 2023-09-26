create type enum_org_role as enum (
    'admin',
    'owner',
    'member'
);

alter table orgs_users add column role enum_org_role;
alter table users add column is_blockjoy_admin boolean not null default false;


update orgs_users as ou
set role = 'owner'::enum_org_role
from (
    select user_id, org_id from user_roles
    where role in ('org-owner', 'org-personal')
) as q
where ou.user_id = q.user_id
and ou.org_id = q.org_id
and ou.role is null;

update orgs_users as ou
set role = 'admin'::enum_org_role
from (
    select user_id, org_id from user_roles
    where role = 'org-admin'
) as q
where ou.user_id = q.user_id
and ou.org_id = q.org_id
and ou.role is null;

update orgs_users as ou
set role = 'member'::enum_org_role
from (
    select user_id, org_id from user_roles
    where role = 'org-member'
) as q
where ou.user_id = q.user_id
and ou.org_id = q.org_id
and ou.role is null;


drop table if exists user_roles;
drop table if exists role_permissions;
drop table if exists roles;
drop table if exists permissions;

