create table feedback (
    feedback_id integer primary key not null,
    name text,
    email text,
    heard_about text,
    feedback text,
    topics text,
    created_at timestamp not null default current_timestamp,
    updated_at timestamp not null default current_timestamp
);

create trigger update_feedback_updated_at
after update on feedback
for each row
begin
    update feedback
    set updated_at = current_timestamp
    where feedback_id = old.feedback_id;
end;
