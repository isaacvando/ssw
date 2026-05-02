create table proposal (
    proposal_id integer primary key not null,
    name text not null,
    email text not null,
    title text not null,
    abstract text not null,
    comments text,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

create trigger update_proposal_updated_at
after update on proposal
for each row
begin
    update proposal
    set updated_at = current_timestamp
    where proposal_id = old.proposal_id;
end;
