use std::num::ParseIntError;

use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use serde_json::value::RawValue;
use sha2::Sha256;
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};
use tracing::info;

use crate::{AppError::*, AppResult};

#[derive(Clone)]
pub struct Stripe {
    pub api_key: String,
    pub webhook_secret: String,
    pub early_bird_price_id: String,
    pub standard_price_id: String,
    pub base_url: String,
    pub client: Client,
}

#[derive(Deserialize)]
pub struct StripeSessionResponse {
    pub id: String,
    pub url: String,
}

impl Stripe {
    pub async fn create_checkout_session(
        &self,
        is_early_bird: bool,
    ) -> AppResult<StripeSessionResponse> {
        let price_id = if is_early_bird {
            &self.early_bird_price_id
        } else {
            &self.standard_price_id
        };
        let expires_at = Utc::now() + Duration::minutes(31);

        let tshirt_size_options: Vec<(String, String)> = TShirtSize::iter()
            .enumerate()
            .flat_map(|(i, size)| {
                [
                    (format!("custom_fields[0][dropdown][options][{i}][label]"), size.external_display_name().into()),
                    (format!("custom_fields[0][dropdown][options][{i}][value]"), size.to_string()),
                ]
            })
            .collect();

        let mut params: Vec<(String, String)> = vec![
            ("success_url".into(), format!("{}/success", self.base_url)),
            ("mode".into(), "payment".into()),
            ("expires_at".into(), expires_at.timestamp().to_string()),
            ("allow_promotion_codes".into(), "true".into()),
            ("line_items[0][price]".into(), price_id.into()),
            ("line_items[0][quantity]".into(), "1".into()),
            ("custom_fields[0][key]".into(), "tshirt_size".into()),
            ("custom_fields[0][label][type]".into(), "custom".into()),
            ("custom_fields[0][label][custom]".into(), "T-shirt size".into()),
            ("custom_fields[0][type]".into(), "dropdown".into()),
            ("custom_fields[0][optional]".into(), "false".into()),
        ];
        params.extend(tshirt_size_options);
        params.extend([
            ("custom_fields[1][key]".into(), "traveling_from".into()),
            ("custom_fields[1][label][type]".into(), "custom".into()),
            ("custom_fields[1][label][custom]".into(), "Where are you traveling from?".into()),
            ("custom_fields[1][type]".into(), "text".into()),
            ("custom_fields[1][optional]".into(), "true".into()),
            ("custom_fields[2][key]".into(), "workplace".into()),
            ("custom_fields[2][label][type]".into(), "custom".into()),
            ("custom_fields[2][label][custom]".into(), "Where do you work?".into()),
            ("custom_fields[2][type]".into(), "text".into()),
            ("custom_fields[2][optional]".into(), "true".into()),
            // Collect the customer's name even if we're giving away a free ticket
            ("name_collection[individual][enabled]".into(), "true".into()),
            ("name_collection[individual][optional]".into(), "false".into()),
        ]);
        let response_str = self
            .client
            .post("https://api.stripe.com/v1/checkout/sessions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .form(&params)
            .send()
            .await?
            .text()
            .await?;
        info!("Stripe checkout session create response:\n{}", response_str);
        let stripe_response: StripeSessionResponse = serde_json::from_str(&response_str)?;
        Ok(stripe_response)
    }

    fn validate_signature(&self, payload: &str, sig: &str) -> Result<(), WebhookError> {
        // Get Stripe signature from header
        let signature = parse_signature(sig)?;
        let signed_payload = format!("{}.{}", signature.t, payload);

        // Compute HMAC with the SHA256 hash function, using endpoint secret as key
        // and signed_payload string as the message.
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.webhook_secret.as_bytes())
            .map_err(|_| WebhookError::BadKey)?;
        mac.update(signed_payload.as_bytes());

        let sig = hex::decode(signature.v1).map_err(|_| WebhookError::BadSignature)?;
        mac.verify_slice(sig.as_slice())
            .map_err(|_| WebhookError::BadSignature)?;

        if (Utc::now().timestamp() - signature.t).abs() > 300 {
            return Err(WebhookError::BadTimestamp(signature.t));
        }

        Ok(())
    }

    pub fn parse_webhook(&self, payload: &str, signature: &str) -> AppResult<StripeEvent> {
        self.validate_signature(payload, signature)?;
        info!("Processing Stripe webhook\n{}", payload);

        let stripe_event_payload: StripeEventPayload = serde_json::from_str(payload)?;

        match stripe_event_payload.event_type {
            "checkout.session.completed" => {
                let event: CheckoutSessionCompletedPayload =
                    serde_json::from_str(stripe_event_payload.data.object.get())?;
                info!("Deserialized: {:#?}", event);
                let mut traveling_from: Option<String> = None;
                let mut workplace: Option<String> = None;
                let mut tshirt_size_optional: Option<String> = None;
                for field in event.custom_fields {
                    match field {
                        CustomField::Dropdown { key, dropdown } if key == "tshirt_size" => {
                            tshirt_size_optional = Some(dropdown.value.to_string());
                        }
                        CustomField::Text { key, text } if key == "traveling_from" => {
                            traveling_from = text.value;
                        }
                        CustomField::Text { key, text } if key == "workplace" => {
                            workplace = text.value;
                        }
                        _ => {
                            return Err(AssertionFailure(
                                "Unsupported field type in checkout session completed payload"
                                    .into(),
                            ));
                        }
                    }
                }

                let tshirt_size = tshirt_size_optional
                    .ok_or(AssertionFailure("Tshirt size is a required field".into()))?;

                let mut promo_code_id: Option<String> = None;
                match &event.discounts[..] {
                    [] => {} // no discount used, do nothing
                    [discount] => promo_code_id = Some(discount.promotion_code.to_string()),
                    _ => {
                        return Err(AssertionFailure(
                            "More than one discount codes found".into(),
                        ));
                    }
                }

                Ok(StripeEvent::CheckoutSessionCompleted(
                    CheckoutSessionCompleted {
                        id: event.id,
                        name: event.customer_details.name,
                        email: event.customer_details.email,
                        tshirt_size,
                        traveling_from,
                        workplace,
                        subtotal: event.amount_subtotal as i64,
                        total: event.amount_total as i64,
                        promo_code_id,
                    },
                ))
            }
            "checkout.session.expired" => {
                let event: CheckoutSessionExpired =
                    serde_json::from_str(stripe_event_payload.data.object.get())?;
                Ok(StripeEvent::CheckoutSessionExpired(event))
            }
            event_type => {
                return Err(AssertionFailure(format!(
                    "Unsupported event type received! {}",
                    event_type
                )));
            }
        }
    }
}

pub enum StripeEvent {
    CheckoutSessionCompleted(CheckoutSessionCompleted),
    CheckoutSessionExpired(CheckoutSessionExpired),
}

#[derive(Deserialize, Debug)]
pub struct CheckoutSessionExpired {
    pub id: String,
}

pub struct CheckoutSessionCompleted {
    pub id: String,
    pub name: String,
    pub email: String,
    pub tshirt_size: String,
    pub traveling_from: Option<String>,
    pub workplace: Option<String>,
    pub subtotal: i64,
    pub total: i64,
    pub promo_code_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct StripeEventPayload<'a> {
    #[serde(rename = "type")]
    event_type: &'a str,
    #[serde(borrow)]
    data: Data<'a>,
}

#[derive(Deserialize, Debug)]
struct Data<'a> {
    #[serde(borrow)]
    object: &'a RawValue,
}

#[derive(Deserialize, Debug)]
pub struct CheckoutSessionCompletedPayload {
    id: String,
    customer_details: CustomerDetails,
    custom_fields: Vec<CustomField>,
    discounts: Vec<Discount>,
    amount_subtotal: u64,
    amount_total: u64,
}

#[derive(Deserialize, Debug)]
struct Discount {
    promotion_code: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum CustomField {
    Dropdown {
        key: String,
        dropdown: DropdownValue,
    },
    Text {
        key: String,
        text: TextValue,
    },
}

#[derive(Deserialize, Debug, Display, EnumIter)]
pub enum TShirtSize {
    Small,
    Medium,
    Large,
    XLarge,
    TwoXLarge,
}

impl TShirtSize {
    fn external_display_name(&self) -> &'static str {
        match self {
            TShirtSize::Small => "Small",
            TShirtSize::Medium => "Medium",
            TShirtSize::Large => "Large",
            TShirtSize::XLarge => "X-Large",
            TShirtSize::TwoXLarge => "2X-Large",
        }
    }
}

#[derive(Deserialize, Debug)]
struct DropdownValue {
    value: TShirtSize,
}

#[derive(Deserialize, Debug)]
struct TextValue {
    value: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CustomerDetails {
    email: String,
    name: String,
}

#[derive(Debug)]
pub enum WebhookError {
    BadKey,
    MissingSignature,
    BadSignature,
    BadHeader(ParseIntError),
    BadTimestamp(i64),
}
impl std::fmt::Display for WebhookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for WebhookError {}

#[derive(Debug)]
struct Signature<'r> {
    t: i64,
    v1: &'r str,
}

fn parse_signature<'r>(raw: &'r str) -> Result<Signature<'r>, WebhookError> {
    let mut t: Option<i64> = None;
    let mut v1: Option<&'r str> = None;
    for pair in raw.split(',') {
        let (key, val) = pair.split_once('=').ok_or(WebhookError::BadSignature)?;
        match key {
            "t" => {
                t = Some(val.parse().map_err(WebhookError::BadHeader)?);
            }
            "v1" => {
                v1 = Some(val);
            }
            _ => {}
        }
    }
    Ok(Signature {
        t: t.ok_or(WebhookError::BadSignature)?,
        v1: v1.ok_or(WebhookError::BadSignature)?,
    })
}
