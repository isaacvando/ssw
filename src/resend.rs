use std::{error::Error, str::FromStr, time::Duration};

use reqwest::{Client, multipart};
use serde::Deserialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use tokio::time::sleep;
use tracing::info;

pub async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let db_options = SqliteConnectOptions::from_str(&crate::env_var("DATABASE_URL"))?
        .read_only(true)
        .pragma("busy_timeout", "5000");
    let pool = SqlitePool::connect_with(db_options).await?;
    let emails = sqlx::query_scalar::<_, String>(
        r#"
        with candidates as (
            select lower(trim(email)) as email from interested_user
            union
            select lower(trim(email)) as email from attendee
        )
        select email
        from candidates
        where not exists (
            select 1 from interested_user
            where lower(trim(interested_user.email)) = candidates.email
            and unsubscribed_at is not null
        )
        order by email;
        "#,
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;

    info!("Selected {} emails for Resend", emails.len());
    if emails.is_empty() {
        return Ok(());
    }

    let mut csv = String::from("email\r\n");
    for email in emails {
        csv.push('"');
        csv.push_str(&email.replace('"', "\"\""));
        csv.push_str("\"\r\n");
    }
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let api_key = crate::env_var("RESEND_API_KEY");

    let file = multipart::Part::text(csv)
        .file_name("ssw-contacts.csv")
        .mime_str("text/csv")?;
    let form = multipart::Form::new()
        .part("file", file)
        .text("column_map", r#"{"email":"email"}"#)
        // Existing contacts, including unsubscribes, must remain untouched.
        .text("on_conflict", "skip")
        .text(
            "segments",
            r#"[{"id":"ab6faf24-f1c8-4284-b1d8-6ff4c257375b"}]"#,
        );
    let response = client
        .post("https://api.resend.com/contacts/imports")
        .bearer_auth(&api_key)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;
    let created: CreatedImport = serde_json::from_slice(&response.bytes().await?)?;
    info!("Submitted Resend contact import {}", created.id);

    loop {
        let response = client
            .get(format!(
                "https://api.resend.com/contacts/imports/{}",
                created.id
            ))
            .bearer_auth(&api_key)
            .send()
            .await?
            .error_for_status()?;
        let import: Import = serde_json::from_slice(&response.bytes().await?)?;
        match import.status.as_str() {
            "completed" => {
                let counts = import.counts.ok_or("Completed import has no counts")?;
                info!(
                    "Resend import {} completed: {} created, {} skipped, {} failed",
                    created.id, counts.created, counts.skipped, counts.failed
                );
                if counts.failed > 0 {
                    return Err(format!(
                        "Resend import {} failed to import {} rows",
                        created.id, counts.failed
                    )
                    .into());
                }
                return Ok(());
            }
            "queued" | "in_progress" => sleep(Duration::from_secs(5)).await,
            "failed" => {
                return Err(format!("Resend import {} failed", created.id).into());
            }
            status => {
                return Err(format!("Unexpected Resend import status: {status}").into());
            }
        }
    }
}

#[derive(Deserialize)]
struct CreatedImport {
    id: String,
}

#[derive(Deserialize)]
struct Import {
    status: String,
    counts: Option<ImportCounts>,
}

#[derive(Deserialize)]
struct ImportCounts {
    created: u64,
    skipped: u64,
    failed: u64,
}
