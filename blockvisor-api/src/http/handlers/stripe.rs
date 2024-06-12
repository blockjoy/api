//! Handlers for incoming stripe webhook events.
//!
//! These are currently only used for follow-up actions after the cancellation
//! of a subscription.

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use axum::routing::{post, Router};
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tracing::{debug, error};

use crate::auth::resource::{OrgId, UserId};
use crate::config::Context;
use crate::database::{Transaction, WriteConn};
use crate::http::response::{bad_params, failed, ok_custom};
use crate::models;
use crate::stripe::api::event;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Stripe database error: {0}
    Database(#[from] diesel::result::Error),
    /// Stripe subscription: {0}
    Stripe(#[from] crate::stripe::Error),
    /// Stripe subscription: {0}
    Subscription(#[from] crate::models::subscription::Error),
    /// Stripe event has an unparsable org_id in its metadata.
    BadOrgId(<OrgId as std::str::FromStr>::Err),
    /// Stripe event has an unparsable user_id in its metadata.
    BadUserId(<UserId as std::str::FromStr>::Err),
    /// Stripe event is missing the metadata field.
    MissingMetadata,
    /// Stripe event is missing a org_id in its metadata.
    MissingOrgId,
    /// Stripe event is missing a user_id in its metadata.
    MissingUserId,
    /// Stripe org: {0}
    Org(#[from] crate::models::org::Error),
}

impl From<Error> for tonic::Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("Stripe webhook: {err:?}");
        match err {
            MissingMetadata => tonic::Status::invalid_argument("Metadata field not set"),
            MissingUserId => tonic::Status::invalid_argument("User id missing from metadata"),
            BadUserId(_) => tonic::Status::invalid_argument("Could not parse user id"),
            MissingOrgId => tonic::Status::invalid_argument("Org id missing from metadata"),
            BadOrgId(_) => tonic::Status::invalid_argument("Could not parse org id"),
            Database(_) | Subscription(_) | Org(_) | Stripe(_) => {
                tonic::Status::internal("Internal error.")
            }
        }
    }
}

pub fn router<S>(context: Arc<Context>) -> Router<S>
where
    S: Clone + Send + Sync,
{
    Router::new()
        .route("/setup_intent_succeeded", post(setup_intent_succeeded))
        .with_state(context)
}

async fn setup_intent_succeeded(State(ctx): State<Arc<Context>>, body: String) -> Response {
    // FIXME: this bastard needs auth.

    let event: event::Event = match serde_json::from_str(&body) {
        Ok(body) => body,
        Err(err) => {
            error!("Failed to parse stripe callback body `{body}`: {err:?}");
            return bad_params();
        }
    };

    let resp = match event.data.object {
        event::EventObject::SetupIntent(data) => {
            ctx.write(|c| setup_intent_succeeded_handler(data, c).scope_boxed())
                .await
        }
        event::EventObject::Other => {
            debug!("Skipping chargebee callback event: {body}");
            return ok_custom("event ignored");
        }
    };

    resp.map_or_else(|_| failed(), |resp| ok_custom(resp.into_inner()))
}

async fn setup_intent_succeeded_handler(
    setup_intent: event::SetupIntent,
    mut write: WriteConn<'_, '_>,
) -> Result<&'static str, Error> {
    println!("Got this setup_intent:");
    println!("{setup_intent:?}");
    let stripe = &write.ctx.stripe;
    let org_id: OrgId = setup_intent
        .metadata
        .ok_or_else(|| Error::MissingMetadata)?
        .get("org_id")
        .ok_or_else(|| Error::MissingOrgId)?
        .parse()
        .map_err(Error::BadOrgId)?;
    let org = models::Org::by_id(org_id, &mut write).await?;
    if let Some(stripe_customer_id) = org.stripe_customer_id.as_ref() {
        stripe
            .attach_payment_method(&setup_intent.payment_method, stripe_customer_id)
            .await?;
    } else {
        let customer_id = stripe
            .create_customer(&org, &setup_intent.payment_method)
            .await?
            .id;
        org.set_customer_id(&customer_id, &mut write).await?;
    };

    Ok("subscription created")
}
