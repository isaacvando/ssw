# The Software Should Work website
This repo contains the website for [Software Should Work](https://softwareshould.work).

The site is deployed to a $5 [Hetzner](https://www.hetzner.com/) VPS with [Caddy](https://caddyserver.com/) for serving static assets and acting as a reverse proxy and load balancer. The app server is written in [Rust](https://www.rust-lang.org/) using [Axum](https://github.com/tokio-rs/axum). It renders dynamic templates (using [Askama](https://github.com/askama-rs/askama)), manages inventory, and enables ticket sales through [Stripe Checkout](https://stripe.com/payments/checkout). The database is [SQLite](https://www.sqlite.org/) and is backed up to [Backblaze B2](https://www.backblaze.com/cloud-storage) (S3 compatible object storage) using [Litestream](https://litestream.io/). [Cloudflare](https://www.cloudflare.com/) is used for DNS and as a reverse proxy. [Grafana Cloud](https://grafana.com/products/cloud/) is used for logs, metrics, alerts, and healthchecks. [RWX](https://www.rwx.com/) is used for CI/CD. For more details, check out the [blog post](https://isaacvando.com/drinking-the-vps-koolaid).

## Running locally
You'll need [Caddy](https://caddyserver.com/) and the [Rust toolchain](https://rustup.rs/) installed to run the site locally. Run `./dev.sh` to start Caddy and the app server with automatic rebuilding.

## Contributing
I probably won't accept any drive-by PRs, but I'm open to suggestions for improving things!

## Misc
The icons are from [Lucide](https://lucide.dev/).

