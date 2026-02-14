create table ticket (
    ticket_id integer primary key not null,
    status text not null,
    stripe_checkout_session_id text unique,
    locked_at timestamp,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

create table attendee (
    attendee_id integer primary key not null,
    ticket_id integer not null unique references ticket(ticket_id),
    name text not null,
    email text not null,
    tshirt_size text null,
    traveling_from text,
    workplace text,
    subtotal integer not null,
    total integer not null,
    stripe_promo_code_id text,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);


create trigger update_ticket_updated_at
after update on ticket
for each row
begin
    update ticket 
    set updated_at = current_timestamp 
    where ticket_id = old.ticket_id;
end;

create trigger update_attendee_updated_at
after update on attendee
for each row
begin
    update attendee
    set updated_at = current_timestamp 
    where attendee_id = old.attendee_id;
end;

with recursive counts(x) as (
    select 1
    union all
    select x + 1 from counts where x < 200
)
insert into ticket (status)
select 'Available' from counts;
