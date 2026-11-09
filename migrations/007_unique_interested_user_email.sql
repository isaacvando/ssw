with duplicates as (
    select
        min(interested_user_id) as keep_id,
        min(created_at) as first_created_at,
        min(unsubscribed_at) as first_unsubscribed_at
    from interested_user
    group by lower(trim(email))
    having count(*) > 1
)
update interested_user
set
    created_at = (
        select first_created_at from duplicates
        where keep_id = interested_user.interested_user_id
    ),
    unsubscribed_at = (
        select first_unsubscribed_at from duplicates
        where keep_id = interested_user.interested_user_id
    )
where interested_user_id in (select keep_id from duplicates);

delete from interested_user
where interested_user_id not in (
    select min(interested_user_id)
    from interested_user
    group by lower(trim(email))
);

-- Match the endpoint's conflict target, including case and surrounding spaces.
create unique index interested_user_email_unique
on interested_user (lower(trim(email)));
