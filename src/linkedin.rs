use axum::http::StatusCode;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::info;

#[derive(Clone)]
pub struct LinkedIn {
    pub access_token: String,
    pub client: Client,
}

pub struct CheckoutConversion {
    pub checkout_session_id: String,
    pub name: String,
    pub email: String,
    pub total: i64,
    pub ip_address: Option<String>,
}

#[derive(Debug)]
pub enum LinkedInError {
    Request(reqwest::Error),
    Serialize(serde_json::Error),
    Http { status: StatusCode, body: String },
}

impl std::fmt::Display for LinkedInError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkedInError::Request(err) => write!(f, "request failed: {}", err),
            LinkedInError::Serialize(err) => write!(f, "serialization failed: {}", err),
            LinkedInError::Http { status, body } => {
                write!(f, "LinkedIn returned {}: {}", status, body)
            }
        }
    }
}

impl std::error::Error for LinkedInError {}

impl LinkedIn {
    pub async fn send_checkout_conversion(
        &self,
        conversion: CheckoutConversion,
    ) -> Result<(), LinkedInError> {
        let (first_name, last_name) = split_name(&conversion.name);
        let user_info = first_name
            .zip(last_name)
            .map(|(first_name, last_name)| UserInfo {
                first_name,
                last_name,
            });

        let mut user_ids = vec![UserId {
            id_type: "SHA256_EMAIL",
            id_value: sha256_email(&conversion.email),
        }];
        if let Some(ip_address) = conversion.ip_address {
            user_ids.push(UserId {
                id_type: "PLAINTEXT_IP_ADDRESS",
                id_value: ip_address,
            });
        }

        let payload = ConversionEventPayload {
            conversion: "urn:lla:llaPartnerConversion:25908476".into(),
            conversion_happened_at: chrono::Utc::now().timestamp_millis(),
            conversion_value: ConversionValue {
                currency_code: "USD",
                amount: cents_to_decimal_string(conversion.total),
            },
            user: ConversionEventUser {
                user_ids,
                user_info,
            },
            event_id: conversion.checkout_session_id,
        };

        let body = serde_json::to_vec(&payload).map_err(LinkedInError::Serialize)?;
        info!(
            body = %String::from_utf8_lossy(&body),
            "Sending LinkedIn conversion event request"
        );
        let response = self
            .client
            .post("https://api.linkedin.com/rest/conversionEvents")
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Content-Type", "application/json")
            .header("Linkedin-Version", "202605")
            .header("X-Restli-Protocol-Version", "2.0.0")
            .body(body)
            .send()
            .await
            .map_err(LinkedInError::Request)?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = response.text().await.map_err(LinkedInError::Request)?;
            Err(LinkedInError::Http { status, body })
        }
    }
}

fn sha256_email(email: &str) -> String {
    let normalized_email: String = email
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(normalized_email.as_bytes());
    hex::encode(hasher.finalize())
}

fn cents_to_decimal_string(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, cents.abs() % 100)
}

fn split_name(name: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = name.split_whitespace().collect();
    match parts.as_slice() {
        [] => (None, None),
        [_] => (None, None),
        [first, rest @ ..] => (Some((*first).into()), Some(rest.join(" "))),
    }
}

#[derive(Serialize)]
struct ConversionEventPayload {
    conversion: String,
    #[serde(rename = "conversionHappenedAt")]
    conversion_happened_at: i64,
    #[serde(rename = "conversionValue")]
    conversion_value: ConversionValue,
    user: ConversionEventUser,
    #[serde(rename = "eventId")]
    event_id: String,
}

#[derive(Serialize)]
struct ConversionValue {
    #[serde(rename = "currencyCode")]
    currency_code: &'static str,
    amount: String,
}

#[derive(Serialize)]
struct ConversionEventUser {
    #[serde(rename = "userIds")]
    user_ids: Vec<UserId>,
    #[serde(rename = "userInfo", skip_serializing_if = "Option::is_none")]
    user_info: Option<UserInfo>,
}

#[derive(Serialize)]
struct UserId {
    #[serde(rename = "idType")]
    id_type: &'static str,
    #[serde(rename = "idValue")]
    id_value: String,
}

#[derive(Serialize)]
struct UserInfo {
    #[serde(rename = "firstName")]
    first_name: String,
    #[serde(rename = "lastName")]
    last_name: String,
}
