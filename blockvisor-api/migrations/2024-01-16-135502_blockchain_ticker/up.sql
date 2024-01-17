alter table blockchains add column ticker text;

update blockchains set ticker = 'TEMP';

alter table blockchains alter column ticker set not null;
