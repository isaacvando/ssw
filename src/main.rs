use askama::Template;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::{get, post},
};
use chrono::{NaiveDate, Utc};
use chrono_tz::America::Chicago;
use serde::Deserialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::env;
use std::net::SocketAddr;
use std::str::FromStr;
mod stripe;
use strum_macros::Display;
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::stripe::{
    CheckoutSessionCompleted, CheckoutSessionExpired, Stripe, StripeEvent, WebhookError,
};

enum AppError {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    Webhook(WebhookError),
    DbError(sqlx::Error),
    Validation(ValidationError),
    AssertionFailure(String),
    TemplateError(askama::Error),
}
use AppError::*;

#[derive(Display)]
enum ValidationError {
    TicketsNoLongerOnSale,
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate {
    message: String,
    static_asset_hash: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Webhook(err) => {
                let msg = match &err {
                    WebhookError::BadHeader(e) => format!("BadHeader: {}", e),
                    WebhookError::BadTimestamp(t) => format!("BadTimestamp: {}", t),
                    _ => format!("{:?}", err),
                };
                warn!("Webhook validation failed: {}", msg);
                (
                    StatusCode::BAD_REQUEST,
                    "Invalid webhook signature".to_string(),
                )
            }
            Reqwest(err) => {
                error!("Reqwest error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Serde(err) => {
                error!("Deserialization error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            DbError(err) => {
                error!("Database error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            TemplateError(e) => {
                error!("A template failed to render {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AssertionFailure(msg) => {
                error!("An assertion failed! {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Validation(err) => {
                info!("Validation error: {}", err);
                let message = match err {
                    ValidationError::TicketsNoLongerOnSale => "Tickets are no longer on sale",
                };
                (StatusCode::BAD_REQUEST, format!("Bad request: {}", message))
            }
        };

        // I don't like reading the env var again here. I would rather read it from the Env like elsewhere, but Axum does not seem to make that easy to do.
        let template = ErrorTemplate {
            message: error_message,
            static_asset_hash: env::var("STATIC_ASSET_HASH").unwrap_or_else(|_| "dev".to_string()),
        };
        match template.render() {
            Ok(html) => (status, Html(html)).into_response(),
            Err(e) => {
                error!("Failed to render error template: {}", e);
                (status, "An error occurred").into_response()
            }
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e)
    }
}
impl From<WebhookError> for AppError {
    fn from(e: WebhookError) -> Self {
        Self::Webhook(e)
    }
}
impl From<sqlx::error::Error> for AppError {
    fn from(e: sqlx::error::Error) -> Self {
        Self::DbError(e)
    }
}
impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        Self::TemplateError(e)
    }
}

type AppResult<T> = Result<T, AppError>;

#[derive(Clone)]
struct Env {
    pool: SqlitePool,
    stripe: Stripe,
    static_asset_hash: String,
}

fn env_var(name: &'static str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{} must be set", name))
}

#[tokio::main]
async fn main() {
    let is_prod = !env::var("ENVIRONMENT").is_ok();
    // log JSON in prod, not in dev
    if is_prod {
        tracing_subscriber::registry()
            .with(EnvFilter::new("info,tower_http=debug"))
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(EnvFilter::new("info,tower_http=debug"))
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    let db_options = SqliteConnectOptions::from_str(&env_var("DATABASE_URL"))
        .expect("Could not find the DB at that path")
        .pragma("foreign_keys", "ON")
        .pragma("busy_timeout", "5000") // https://litestream.io/tips/#busy-timeout
        .pragma("journal_mode", "WAL");

    let pool = SqlitePool::connect_with(db_options)
        .await
        .expect("Unable to connect to DB");

    let stripe = Stripe {
        api_key: env_var("STRIPE_API_KEY"),
        webhook_secret: env_var("STRIPE_WEBHOOK_SECRET"),
        early_bird_price_id: env_var("EARLY_BIRD_PRICE_ID"),
        standard_price_id: env_var("STANDARD_PRICE_ID"),
        base_url: env_var("BASE_URL"),
        client: reqwest::Client::new(),
    };
    let env = Env {
        pool,
        stripe,
        static_asset_hash: env::var("STATIC_ASSET_HASH").unwrap_or_else(|_| "dev".to_string()),
    };
    let app = Router::new()
        .route("/", get(home))
        .route("/checkout", post(checkout))
        .route("/success", get(success))
        .route("/sold_out", get(sold_out))
        .route("/stripe-webhook", post(stripe_webhook))
        .route("/interested", post(interested_user))
        .route("/healthcheck", get(healthcheck))
        .route("/policies", get(policies))
        .fallback(handler_404)
        .with_state(env)
        .layer(TraceLayer::new_for_http());

    let port = u16::from_str(&env_var("PORT")).expect("The PORT value is invalid");

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn healthcheck(State(env): State<Env>) -> AppResult<&'static str> {
    _ = sqlx::query("select 1").execute(&env.pool).await?;
    Ok("Healthy")
}

async fn home(State(env): State<Env>) -> AppResult<Html<String>> {
    home_html(env, false, false).await
}

async fn success(State(env): State<Env>) -> AppResult<Html<String>> {
    home_html(env, true, false).await
}

async fn sold_out(State(env): State<Env>) -> AppResult<Html<String>> {
    home_html(env, false, true).await
}

#[derive(Template)]
#[template(path = "index.html")]
struct HomeTemplate {
    is_early_bird: bool,
    is_sold_out: bool,
    show_success_banner: bool,
    show_sold_out_banner: bool,
    static_asset_hash: String,
}

async fn home_html(
    env: Env,
    show_success_banner: bool,
    show_sold_out_banner: bool,
) -> AppResult<Html<String>> {
    let claimed = sqlx::query!(
        r#"
           select count(*) as claimed
           from ticket
           where status = 'Pending' or status = 'Sold' 
        "#
    )
    .fetch_one(&env.pool)
    .await?
    .claimed;
    let is_early_bird = is_early_bird(claimed);

    // Check if we've sold out. If it's already early bird we know there are tickets remaining and don't need to hit the DB.
    let is_sold_out = !is_early_bird
        && sqlx::query!(
            r#"
        select not exists (
            select 1
            from ticket
            where status = 'Available'
        ) as "sold_out: bool"
    "#,
        )
        .fetch_one(&env.pool)
        .await?
        .sold_out;

    let template = HomeTemplate {
        is_early_bird,
        is_sold_out,
        show_success_banner,
        show_sold_out_banner,
        static_asset_hash: env.static_asset_hash,
    };
    Ok(template.render()?.into())
}

fn is_early_bird(claimed: i64) -> bool {
    let today = Utc::now().with_timezone(&Chicago).date_naive();
    today <= NaiveDate::from_ymd_opt(2026, 3, 13).expect("Invalid date") && claimed <= 100
}

#[derive(Template)]
#[template(path = "policies.html")]
struct PoliciesTemplate {
    static_asset_hash: String,
}
async fn policies(State(env): State<Env>) -> AppResult<Html<String>> {
    let template = PoliciesTemplate {
        static_asset_hash: env.static_asset_hash,
    };
    Ok(template.render()?.into())
}

async fn checkout(State(env): State<Env>, headers: HeaderMap) -> AppResult<Redirect> {
    if Utc::now().date_naive() > NaiveDate::from_ymd_opt(2026, 7, 15).expect("Invalid date") {
        return Err(Validation(ValidationError::TicketsNoLongerOnSale));
    };

    // Reserve a ticket if there is one available. Atomically within the same statement, count the number of claimed tickets so we can determine if early bird is still available.
    let reservation_result = sqlx::query!(
        r#"
        update ticket
        set status = 'Pending',
        locked_at = current_timestamp
        where ticket_id = (
            select ticket_id
            from ticket
            where status = 'Available'
            and locked_at is null
            limit 1
        )
        returning ticket_id, (
            select count(*)
            from ticket
            where status in ('Sold', 'Pending')
        ) as claimed
        "#
    )
    .fetch_optional(&env.pool)
    .await?;

    let reservation = match reservation_result {
        Some(res) => res,
        None => {
            warn!("A user requested to open a checkout session but tickets are sold out");
            return Ok(Redirect::to("/sold_out".into()));
        }
    };
    info!(
        "Reserved ticket {}. Total claimed: {}",
        reservation.ticket_id, reservation.claimed
    );

    let is_early_bird = is_early_bird(reservation.claimed);
    // If we crash here, there will be a locked ticket with no associated checkout session.
    // I'm not worried about this; if it does happen I'll clean it up manually or set up a reaper process.
    let response = match env.stripe.create_checkout_session(is_early_bird).await {
        Ok(resp) => resp,
        Err(err) => {
            error!(
                "Unable to create stripe checkout session for ticket {}. Releasing lock.",
                reservation.ticket_id
            );
            sqlx::query!(
                r#"
                update ticket
                set status = 'Available',
                locked_at = null
                where ticket_id = ?
                "#,
                reservation.ticket_id
            )
            .execute(&env.pool)
            .await?;
            return Err(err);
        }
    };

    let ip_address = headers
        .get("CF-Connecting-IP")
        .and_then(|h| h.to_str().ok());

    let result = sqlx::query!(
        r#"
          update ticket
          set stripe_checkout_session_id = ?, ip_address = ?
          where ticket_id = ?  
        "#,
        response.id,
        ip_address,
        reservation.ticket_id
    )
    .execute(&env.pool)
    .await?;
    info!(
        "Set checkout session id on ticket {}",
        reservation.ticket_id
    );
    assert(
        result.rows_affected() == 1,
        "The checkout session id should be set on a single ticket once the session is created",
    )?;
    Ok(Redirect::to(&response.url))
}

async fn stripe_webhook(
    State(env): State<Env>,
    headers: HeaderMap,
    body: String,
) -> AppResult<StatusCode> {
    let signature = headers
        .get("Stripe-Signature")
        .ok_or(WebhookError::MissingSignature)?
        .to_str()
        .map_err(|_| WebhookError::BadSignature)?;

    let stripe_event = env.stripe.parse_webhook(&body, signature)?;
    match stripe_event {
        StripeEvent::CheckoutSessionCompleted(CheckoutSessionCompleted {
            id,
            name,
            email,
            tshirt_size,
            traveling_from,
            workplace,
            total,
            subtotal,
            promo_code_id,
        }) => {
            let ticket_id = sqlx::query!(
                r#"
                select ticket_id
                from ticket
                where stripe_checkout_session_id = ?
                and status = 'Pending'
                "#,
                id
            )
            .fetch_optional(&env.pool)
            .await?
            .ok_or(AssertionFailure(format!("No ticket found with id {}", id)))?
            .ticket_id;

            let mut tx = env.pool.begin().await?;
            let attendee_id = sqlx::query!(
                r#"
            insert into attendee (ticket_id, name, email, tshirt_size, traveling_from, workplace, subtotal, total, stripe_promo_code_id)
            values (?, ?, ?, ?, ?, ?, ?, ?, ?) returning attendee_id   
            "#,
                ticket_id,
                name,
                email,
                tshirt_size,
                traveling_from,
                workplace,
                subtotal,
                total,
                promo_code_id,
            ).fetch_one(&mut *tx)
            .await?
            .attendee_id;
            info!("Inserted attendee {}", attendee_id);

            let result = sqlx::query!(
                r#"
                update ticket
                set status = 'Sold'
                where ticket_id = ?
                and status = 'Pending' -- Only pending tickets should become sold
                "#,
                ticket_id
            )
            .execute(&mut *tx)
            .await?;
            info!("Updated ticket {} to sold", ticket_id);
            tx.commit().await?;

            assert(
                result.rows_affected() == 1,
                "A single ticket should be marked sold when a checkout session completes",
            )?;

            Ok(StatusCode::OK)
        }
        StripeEvent::CheckoutSessionExpired(CheckoutSessionExpired { id }) => {
            let ticket_id = sqlx::query!(
                r#"
                update ticket
                set status = 'Available', locked_at = null, stripe_checkout_session_id = null, ip_address = null
                where stripe_checkout_session_id = ?
                and status = 'Pending' -- Only pending tickets should become available
                returning ticket_id
                "#,
                id
            )
            .fetch_one(&env.pool)
            .await?
            .ticket_id;

            info!(
                "Expired lock on ticket {} for checkout session {}",
                ticket_id, id
            );

            Ok(StatusCode::OK)
        }
    }
}

#[derive(Deserialize)]
struct InterestedUserRequest {
    email: String,
}

async fn interested_user(
    State(env): State<Env>,
    Json(payload): Json<InterestedUserRequest>,
) -> AppResult<StatusCode> {
    sqlx::query!(
        r#"
        insert into interested_user (email)
        values (?)
        "#,
        payload.email
    )
    .execute(&env.pool)
    .await?;

    info!("Added interested user: {}", payload.email);

    Ok(StatusCode::OK)
}

async fn handler_404() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Signal received, starting graceful shutdown...");
}

#[must_use]
fn assert(condition: bool, message: &str) -> AppResult<()> {
    if !condition {
        Err(AssertionFailure(message.into()))
    } else {
        Ok(())
    }
}
