select email
from interested_user
where unsubscribed_at is null

union

select email
from attendee
where email <> 'tbd';