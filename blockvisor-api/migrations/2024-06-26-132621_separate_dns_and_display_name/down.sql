alter table nodes drop column dns_name;
alter table nodes drop column display_name;
alter table nodes rename column node_name to name;
