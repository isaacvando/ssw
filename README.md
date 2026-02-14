# The Software Should Work website
This repo contains the website for [Software Should Work](https://softwareshould.work).

The site is deployed to a $5 Hetzner VPS with Caddy for serving static assets and acting as a reverse proxy and load balancer. The app server is written in Rust using Axum. It renders dynamic templates (using Askama), manages inventory, and enables ticket sales through Stripe Checkout. The database is SQLite and is backed up to Backblaze B2 (S3 compatible object storage) using Litestream. Cloudflare is used for DNS and as a reverse proxy. Grafana Cloud is used for logs, metrics, alerts, and healthchecks. RWX is used for CI/CD.

## Running locally
You'll need [Caddy](https://caddyserver.com/) and the [Rust toolchain](https://rustup.rs/) installed to run the site locally. Run `./dev.sh` to start caddy and the app server with automatic rebuilding.

## Contributing
I probably won't accept any drive-by PRs, but I'm open to suggestions for improving things!

## Misc
The icons are from [Lucide](https://lucide.dev/).

