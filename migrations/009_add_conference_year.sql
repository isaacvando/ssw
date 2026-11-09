alter table ticket
add column conference_year integer not null default 2026;

alter table proposal
add column conference_year integer not null default 2026;

alter table feedback
add column conference_year integer not null default 2026;
