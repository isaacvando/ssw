#!/bin/bash

set -euo pipefail

echo "Setting up SSH"
mkdir -p ~/.ssh
echo "$SSH_PRIVATE_KEY" > ~/.ssh/ci
chmod 600 ~/.ssh/ci
cat >> ~/.ssh/config <<EOF
Host server
    HostName $SERVER_IP_ADDRESS
    User root
    IdentityFile ~/.ssh/ci
    StrictHostKeyChecking no
EOF


echo "Copying static files"
rsync -av --delete static server:/

echo "Setting secrets"
ssh server "umask 077 && cat > /root/secrets.env" <<EOF
STRIPE_API_KEY=$STRIPE_API_KEY
STRIPE_WEBHOOK_SECRET=$STRIPE_WEBHOOK_SECRET
EOF

echo "Setting up Alloy"
scp config.alloy server:/etc/alloy
ssh server "umask 077 && cat > /etc/default/alloy" <<EOF
CONFIG_FILE="/etc/alloy/config.alloy"
GRAFANA_CLOUD_API_KEY=$GRAFANA_API_KEY
EOF
ssh server 'systemctl restart alloy && systemctl is-active alloy --quiet'

echo "Setting up Litestream"
ssh server "umask 077 && cat > /etc/litestream.yml" <<EOF
access-key-id: $B2_KEY_ID
secret-access-key: $B2_SECRET_KEY

dbs:
  - path: /root/ssw.db
    replica:
      type: s3
      bucket: sswork
      path: litestream
      endpoint: s3.us-east-005.backblazeb2.com
      force-path-style: true
      sync-interval: 1m
      sign-payload: true # avoids recent bug due to S3 header not supported by all S3 compatible vendors
      l0-retention-check-interval: 10m
EOF
ssh server 'systemctl restart litestream && systemctl is-active litestream --quiet'

echo "Applying DB migrations"
./migrate.sh server

echo "Copying service files"
scp blue.env server:/root
scp green.env server:/root
scp ssw@.service server:/etc/systemd/system

echo "Computing static asset hash"
STATIC_ASSET_HASH=$(find static -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-8)
echo "Static asset hash: $STATIC_ASSET_HASH"

echo "Writing config.env"
ssh server "cat > /root/config.env" <<EOF
$(cat config.env)
STATIC_ASSET_HASH=$STATIC_ASSET_HASH
EOF

ssh server 'rm ssw'
scp target/release/ssw server:/root

echo "Beginning blue-green deployment"
if ssh server "systemctl is-active ssw@blue --quiet"; then
    OLD_COLOR="blue"
    NEW_COLOR="green"
    NEW_PORT="3001"
else
    OLD_COLOR="green"
    NEW_COLOR="blue"
    NEW_PORT="3000"
fi
echo "Deploying $NEW_COLOR on $NEW_PORT"
ssh server 'systemctl daemon-reload'
ssh server "systemctl restart ssw@$NEW_COLOR && systemctl is-active ssw@$NEW_COLOR --quiet"

# Give the new service a chance to start up
sleep 2

if ssh server "curl -s -f http://localhost:$NEW_PORT/healthcheck > /dev/null"; then
  echo "Service is healthy, reloading caddy"
  sed "s/localhost:3000/localhost:$NEW_PORT/g" Caddyfile > Caddyfile.deploy
  scp Caddyfile.deploy server:/etc/caddy/Caddyfile 
  ssh server 'systemctl reload caddy && systemctl is-active caddy --quiet'

  echo "Purging Cloudflare cache"
  curl -X POST "https://api.cloudflare.com/client/v4/zones/$CLOUDFLARE_ZONE/purge_cache" \
    -H "Authorization: Bearer $CLOUDFLARE_API_KEY" \
    -H "Content-Type: application/json" \
    --data '{"purge_everything":true}'

  echo "Waiting for load balancer to drain..."
  sleep 5

  echo "Stopping old service"
  ssh server "systemctl is-active ssw@$OLD_COLOR --quiet && systemctl stop ssw@$OLD_COLOR"
else
  echo "Service is not healthy"
  exit 1
fi
