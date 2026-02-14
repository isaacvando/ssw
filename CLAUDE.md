# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

SSW is a Rust-based conference ticketing backend for the Software Should Work conference (July 16-17, 2026). Built with Axum web framework, SQLite database, and Stripe for payments.

## Common Commands

```bash
# Development - starts Caddy proxy + cargo watch with hot reload
./dev.sh

# Run database migrations
./migrate.sh local      # local development
./migrate.sh server     # production

# Build release binary
cargo build --release

# CI script to deploy to production (blue-green deployment)
./deploy.sh
```

## Architecture

**Tech Stack:** Rust 2024 Edition, Axum 0.8, SQLx (SQLite), Askama templates, Stripe API

**Key Files:**
- `src/main.rs` - Routes, handlers, application state, error handling
- `src/stripe.rs` - Stripe checkout sessions and webhook processing
- `templates/index.html` - Askama HTML template for the ticket page
- `static/styles.css` - Styling
- `migrations/001_init.sql` - Database schema (ticket + attendee tables)

**Application Flow:**
1. User visits `/` → renders ticket page with availability status
2. POST `/checkout` → atomically reserves ticket, creates Stripe session, redirects
3. Stripe webhook at `/stripe-webhook` → marks ticket sold or releases on expiration
4. Early bird pricing: first 100 tickets before March 15, 2026

**State Management:**
```rust
struct Env {
    pool: SqlitePool,
    stripe: Stripe,
    static_asset_hash: String,
    ...
}
```
Shared via `axum::extract::State` across all handlers.

**Error Handling:** Custom `AppError` enum implementing `IntoResponse` with `AppResult<T>` type alias.

**Database Patterns:**
- Compile-time checked queries via `sqlx::query!` macros
- Atomic reservation: single SQL statement reserves ticket AND counts claimed tickets
- WAL mode enabled, foreign keys enforced

## Deployment

**Infrastructure:** Caddy reverse proxy, Litestream for SQLite backup to Backblaze B2

**Blue-Green Deployment:** Two systemd services (`ssw@blue`, `ssw@green`) on ports 3000/3001. New version starts on alternate port, Caddy switches traffic after health check.

**CI/CD:** RWX platform (`.rwx/tasks.yml`)

## Environment Variables

Required in development (`dev.sh`) and production (`config.env` + `secrets.env`):
- `DATABASE_URL` - SQLite connection string
- `BASE_URL` - Application base URL
- `STRIPE_API_KEY`, `STRIPE_WEBHOOK_SECRET` - Stripe credentials
- `EARLY_BIRD_PRICE_ID`, `STANDARD_PRICE_ID` - Stripe price IDs
