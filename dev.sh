#!/bin/bash

trap "kill 0" EXIT # Kill Caddy when the script exits
caddy run --config Caddyfile.local &

./migrate.sh local

export BASE_URL=http://localhost:8080
export PORT=3000
export EARLY_BIRD_PRICE_ID=price_1Sbpu5P4jNZJNV9kPC7MBYfX
export STANDARD_PRICE_ID=price_1SbptUP4jNZJNV9kkJnfvSLc
export ENVIRONMENT=dev
cargo watch -x run
