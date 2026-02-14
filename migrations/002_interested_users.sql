create table interested_user (
    interested_user_id integer primary key not null,
    email text not null,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

create trigger update_interested_user_updated_at
after update on interested_user
for each row
begin
    update interested_user
    set updated_at = current_timestamp 
    where interested_user_id = old.interested_user_id;
end;
