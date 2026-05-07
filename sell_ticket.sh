#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 NAME EMAIL TSHIRT_SIZE" >&2
  exit 1
fi

quote() {
  printf "'"
  printf "%s" "$1" | sed "s/'/''/g"
  printf "'"
}

name="$(quote "$1")"
email="$(quote "$2")"
tshirt_size="$(quote "$3")"

sqlite3 "ssw.db" <<SQL
.bail on
BEGIN IMMEDIATE;

CREATE TEMP TABLE selected_ticket(ticket_id integer primary key);
INSERT INTO selected_ticket
SELECT ticket_id FROM ticket
WHERE status = 'Available'
ORDER BY ticket_id LIMIT 1;

CREATE TEMP TABLE require_ticket(found integer not null check(found = 1));
INSERT INTO require_ticket SELECT count(*) FROM selected_ticket;

UPDATE ticket
SET status = 'Sold', locked_at = current_timestamp
WHERE ticket_id = (SELECT ticket_id FROM selected_ticket);

INSERT INTO attendee (ticket_id, name, email, tshirt_size, subtotal, total)
SELECT ticket_id, $name, $email, $tshirt_size, 0, 0 FROM selected_ticket;

COMMIT;
SELECT 'Sold ticket ' || ticket_id FROM selected_ticket;
SQL
