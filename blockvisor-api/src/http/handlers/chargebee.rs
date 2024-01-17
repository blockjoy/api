//! Handler for incoming chargebee events.
//!
//! These are currently only used for follow-up actions after the cancellation
//! of a subscription.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::{post, Router};
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use serde::Deserialize;
use serde_enum_str::Deserialize_enum_str;
use thiserror::Error;
use tracing::{debug, error};

use crate::config::Context;
use crate::database::{Transaction, WriteConn};
use crate::grpc::{api, command};
use crate::http::response::{bad_params, failed, not_found, ok_custom};
use crate::models::command::NewCommand;
use crate::models::{CommandType, Node, Subscription};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Chargebee command: {0}
    Command(#[from] crate::models::command::Error),
    /// Chargebee database error: {0}
    Database(#[from] diesel::result::Error),
    /// Chargebee gRPC command: {0}
    GrpcCommand(#[from] crate::grpc::command::Error),
    /// Chargebee IpAddress: {0}
    IpAddress(#[from] crate::models::ip_address::Error),
    /// Chargebee node: {0}
    Node(#[from] crate::models::node::Error),
    /// Chargebee failed to parse IpAddr: {0}
    ParseIpAddr(std::net::AddrParseError),
    /// Chargebee subscription: {0}
    Subscription(#[from] crate::models::subscription::Error),
}

impl From<Error> for tonic::Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("Chargebee webhook: {err:?}");
        match err {
            Command(_) | Database(_) | GrpcCommand(_) | IpAddress(_) | Node(_) | ParseIpAddr(_)
            | Subscription(_) => tonic::Status::internal("Internal error"),
        }
    }
}

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/callback/:secret", post(callback))
        .with_state(context)
}

#[derive(Debug, Deserialize)]
struct Callback {
    content: Content,
    event_type: EventType,
}

#[derive(Debug, Deserialize)]
struct Content {
    subscription: EventSubscription,
}

#[derive(Debug, Deserialize)]
struct EventSubscription {
    id: String,
}

#[derive(Debug, Deserialize_enum_str)]
#[serde(rename_all = "snake_case")]
enum EventType {
    SubscriptionCancelled,
    #[serde(other)]
    Other(String),
}

async fn callback(
    State(ctx): State<Arc<Context>>,
    Path(secret): Path<String>,
    body: String,
) -> Response {
    if ctx.config.chargebee.secret != secret {
        error!("Bad chargebee callback secret. Ignoring event.");
        // We return a 404 if the secret is incorrect, so we don't give away
        // that there is a secret in this url that might be brute-forced.
        return not_found();
    }

    // This is temporary, until we get it working end to end
    // I (luuk) will definitely be going into the logs to inspect these values
    dbg!(&body);

    // We only start parsing the json after the secret is verfied so people
    // can't try to discover this endpoint.
    let callback: Callback = match serde_json::from_str(&body) {
        Ok(body) => body,
        Err(err) => {
            error!("Failed to parse chargebee callback body `{body}`: {err:?}");
            return bad_params();
        }
    };

    dbg!(&callback);

    let resp = match callback.event_type {
        EventType::SubscriptionCancelled => {
            ctx.write(|c| subscription_cancelled(callback, c).scope_boxed())
                .await
        }
        EventType::Other(event) => {
            debug!("Skipping chargebee callback event: {event}");
            return ok_custom("event ignored");
        }
    };

    resp.map_or_else(|_| failed(), |resp| ok_custom(resp.into_inner()))
}

/// When a subscription is cancelled we delete all the nodes associated with
/// that org.
async fn subscription_cancelled(
    callback: Callback,
    mut write: WriteConn<'_, '_>,
) -> Result<&'static str, Error> {
    let id = callback.content.subscription.id;
    let subscription = Subscription::by_external_id(&id, &mut write).await?;
    let nodes = Node::by_org_id(subscription.org_id, &mut write).await?;

    for node in nodes {
        delete_node(&node, &mut write).await?;
    }

    Ok("subscription cancelled")
}

async fn delete_node(node: &Node, write: &mut WriteConn<'_, '_>) -> Result<(), Error> {
    let new_command = NewCommand::node(node, CommandType::DeleteNode)?;
    let cmd = new_command.create(write).await?;

    write.mqtt(command::delete_node(&cmd)?);
    write.mqtt(api::NodeMessage::deleted(node, None));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_example_event() {
        let test_event = r#"{
            "id": "ev_Azqea1ToyBXXnjoP",
            "occurred_at": 1693902804,
            "source": "api",
            "user": "full_access_key_v1",
            "object": "event",
            "api_version": "v2",
            "content": {
                "subscription": {
                    "id": "Azz5eSTos9KFABDRX",
                    "billing_period": 1,
                    "billing_period_unit": "year",
                    "auto_collection": "on",
                    "customer_id": "169luiToJHAWBQHr",
                    "status": "cancelled",
                    "current_term_start": 1693813618,
                    "current_term_end": 1693902804,
                    "created_at": 1693813618,
                    "started_at": 1693813618,
                    "activated_at": 1693813618,
                    "cancelled_at": 1693902804,
                    "updated_at": 1693902804,
                    "has_scheduled_changes": false,
                    "payment_source_id": "pm_Azz5eSTos998aBDIe",
                    "cancel_schedule_created_at": 1693902526,
                    "channel": "web",
                    "resource_version": 1693902804209,
                    "deleted": false,
                    "object": "subscription",
                    "currency_code": "USD",
                    "subscription_items": [
                        {
                            "item_price_id": "STANDARD-USD-Y",
                            "item_type": "plan",
                            "quantity": 1,
                            "unit_price": 0,
                            "amount": 0,
                            "free_quantity": 0,
                            "object": "subscription_item"
                        }
                    ],
                    "due_invoices_count": 0,
                    "mrr": 0,
                    "has_scheduled_advance_invoices": false
                },
                "customer": {
                    "id": "169luiToJHAWBQHr",
                    "first_name": "10Dragan",
                    "last_name": "Rakita",
                    "email": "dragan+10@blockjoy.com",
                    "auto_collection": "on",
                    "net_term_days": 0,
                    "allow_direct_debit": false,
                    "created_at": 1693298315,
                    "taxability": "taxable",
                    "updated_at": 1693861347,
                    "pii_cleared": "active",
                    "channel": "web",
                    "resource_version": 1693861347042,
                    "deleted": false,
                    "object": "customer",
                    "billing_address": {
                        "first_name": "10Dragan",
                        "last_name": "010Rakita",
                        "line1": "339 Pacific Ave.",
                        "city": "Hernando",
                        "country": "US",
                        "zip": "85001",
                        "validation_status": "not_validated",
                        "object": "billing_address"
                    },
                    "card_status": "valid",
                    "contacts": [
                        {
                            "id": "contact_AzqeaYToP5cUKem7",
                            "first_name": "Dragan  Rakita",
                            "email": "dragan@blockjoy.com",
                            "label": "AzZiy3ToJHCIQSrk",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        },
                        {
                            "id": "contact_AzZj5RToP7vrIiqr",
                            "first_name": "Dragan  Rakita2",
                            "email": "dragan+02@blockjoy.com",
                            "label": "AzZiy3ToJHCIQSrk",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        },
                        {
                            "id": "contact_AzqeaYToP88S3gq0",
                            "first_name": "Dragan  Rakita3",
                            "email": "dragan+03@blockjoy.com",
                            "label": "AzZiy3ToJHCIQSrk",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        },
                        {
                            "id": "contact_AzZj5RToP8mAvjdp",
                            "first_name": "Dragan  Rakita4",
                            "email": "dragan+04@blockjoy.com",
                            "label": "AzZiy3ToJHCIQSrk",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        },
                        {
                            "id": "contact_Azz5cbToUiF3bNoc",
                            "first_name": "Dragan  Rakita5",
                            "email": "dragan+05@blockjoy.com",
                            "label": "AzZiy3ToJHCIQSrk",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        },
                        {
                            "id": "contact_Azz5jjTovNad52Gxz",
                            "first_name": "Dragan  Rakita",
                            "email": "dragan@blockjoy.com",
                            "label": "16CJAsToViB8HLTQ",
                            "enabled": true,
                            "send_account_email": false,
                            "send_billing_email": true,
                            "object": "contact"
                        }
                    ],
                    "balances": [
                        {
                            "promotional_credits": 0,
                            "excess_payments": 0,
                            "refundable_credits": 12,
                            "unbilled_charges": 0,
                            "object": "customer_balance",
                            "currency_code": "USD",
                            "balance_currency_code": "USD"
                        }
                    ],
                    "promotional_credits": 0,
                    "refundable_credits": 12,
                    "excess_payments": 0,
                    "unbilled_charges": 0,
                    "preferred_currency_code": "USD",
                    "primary_payment_source_id": "pm_Azz5eSTos998aBDIe",
                    "payment_method": {
                        "object": "payment_method",
                        "type": "card",
                        "reference_id": "CB_AzqeOcTos98L0BE1M/XQ2BP5MZF3M84H82",
                        "gateway": "adyen",
                        "gateway_account_id": "gw_AzZiy6TcWrETcfCo",
                        "status": "valid"
                    },
                    "tax_providers_fields": []
                },
                "card": {
                    "status": "valid",
                    "gateway": "adyen",
                    "gateway_account_id": "gw_AzZiy6TcWrETcfCo",
                    "ref_tx_id": "ZKSS94Z9HVTFWR82",
                    "first_name": "10Dragan",
                    "last_name": "010Rakita",
                    "iin": "555555",
                    "last4": "4444",
                    "card_type": "mastercard",
                    "funding_type": "not_known",
                    "expiry_month": 3,
                    "expiry_year": 2030,
                    "billing_addr1": "339 Pacific Ave.",
                    "billing_addr2": "ZZ",
                    "billing_city": "Hernando",
                    "billing_state": "ZZ",
                    "billing_country": "US",
                    "billing_zip": "85001",
                    "created_at": 1693813575,
                    "updated_at": 1693813575,
                    "powered_by": "card",
                    "resource_version": 1693813575765,
                    "object": "card",
                    "masked_number": "************4444",
                    "customer_id": "169luiToJHAWBQHr",
                    "payment_source_id": "pm_Azz5eSTos998aBDIe"
                }
            },
            "event_type": "subscription_cancelled",
            "webhook_status": "not_configured"
        }"#;
        let _: Callback = serde_json::from_str(test_event).unwrap();
    }
}
