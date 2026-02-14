#!/bin/bash

# Exit immediately if a command exits with a non-zero status.
set -e

target=${1:-local}

# Execute SQL from stdin locally or on the VPS
sql_exec() {
  if [ "$target" == "local" ]; then
    sqlite3 ssw.db
  else
    ssh "$target" "sqlite3 ssw.db"
  fi
}

if [ ! -d "./migrations" ]; then
  echo "Error: Migrations directory not found."
  exit 1
fi

if [ "$target" == "local" ]; then
  echo "Applying migrations locally"
else
  echo "Applying migrations in production"
fi

echo "
create table if not exists migration (
    id integer primary key not null,
    name text not null,
    version int not null,
    created_at timestamp not null default current_timestamp,
    unique(name),
    unique(version)
);
" | sql_exec

latest_version=$(echo "select coalesce(max(version), 0) from migration" | sql_exec)
echo "Latest migration version: $latest_version"

find migrations -type f -name '*.sql' | sort | while read -r file; do
  filename=$(basename $file)
  version_str=$(echo "$filename" | cut -d'_' -f1)
  version_num=$((10#$version_str))
  name=$(echo "$filename" | sed -E -e "s/^${version_str}_(.*)\.sql$/\1/")

  if [ "$version_num" -gt "$latest_version" ]; then
    echo "Applying $filename"
    cat $file | sql_exec
    echo "insert into migration (name, version) values ('$filename', $version_num);" | sql_exec
  fi
done
echo "Successfully applied all migrations"
