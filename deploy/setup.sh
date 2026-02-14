#!/bin/bash

set -euo pipefail

echo "Adding ssh key for CI"
echo "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOsRj3xZoBukhd4JNzrRoOP0haY8SH6sv/yUEz6brb5y ci" >> .ssh/authorized_keys

echo "Configuring UFW"
ufw default deny incoming
ufw allow ssh
ufw allow 80
ufw allow 443
ufw --force enable

echo "Installing packages"
export DEBIAN_FRONTEND=noninteractive
apt update
apt install -y debian-keyring debian-archive-keyring apt-transport-https sqlite3
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor | sudo tee /usr/share/keyrings/caddy-stable-archive-keyring.gpg > /dev/null
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
apt install -y caddy

echo "Installing Litestream"
wget https://github.com/benbjohnson/litestream/releases/download/v0.5.3/litestream-0.5.8-linux-x86_64.deb
dpkg -i litestream-0.5.8-linux-x86_64.deb
