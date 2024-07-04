alter table nodes rename column name to node_name;
alter table nodes add column dns_name text;
alter table nodes add column display_name text;

update nodes set
dns_name = node_name,
display_name = node_name;

alter table nodes alter column dns_name set not null;
alter table nodes alter column display_name set not null;
