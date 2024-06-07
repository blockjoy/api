use std::any::type_name;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::auth::resource::{OrgId, UserId};

/// The SetupIntent message, used by the frontend to add a card to our stripe environment.
/// <https://docs.stripe.com/api/setup_intents/create>
/// There are some field that the documentation specifies as being null, so those are omitted from
/// our message.
#[derive(Deserialize)]
#[allow(unused)]
pub struct SetupIntent {
    id: String,
    object: String,
    pub client_secret: String,
    created: i64,
    livemode: bool,
    metadata: super::Metadata,
    payment_method_options: PaymentMethodOptions,
    payment_method_types: Vec<String>,
    status: String,
    usage: String,
}

#[derive(Deserialize)]
#[allow(unused)]
struct PaymentMethodOptions {
    card: Card,
}

#[derive(Deserialize)]
#[allow(unused)]
struct Card {
    request_three_d_secure: String,
}

/// Creates a SetupIntent object.
///
/// After you create the SetupIntent, attach a payment method and confirm it to collect any required
/// permissions to charge the payment method later.
///
/// <https://docs.stripe.com/api/setup_intents/create>
#[derive(Serialize, Default)]
pub struct CreateSetupIntent<'a> {
    /// Set to true to attempt to confirm this SetupIntent immediately. This parameter defaults to
    /// false. If a card is the attached payment method, you can provide a return_url in case
    /// further authentication is necessary.
    confirm: Option<bool>,
    /// ID of the Customer this SetupIntent belongs to, if one exists.
    ///
    /// If present, the SetupIntent’s payment method will be attached to the Customer on
    /// successful setup. Payment methods attached to other Customers cannot be used with this
    /// SetupIntent.
    customer: Option<&'a str>,
    /// An arbitrary string attached to the object. Often useful for displaying to users.
    description: Option<&'a str>,
    /// Set of key-value pairs that you can attach to an object. This can be useful for storing
    /// additional information about the object in a structured format. Individual keys can be unset
    /// by posting an empty value to them. All keys can be unset by posting an empty value to
    /// metadata.
    metadata: Option<super::Metadata>,
    /// ID of the payment method (a PaymentMethod, Card, or saved Source object) to attach to this
    /// SetupIntent.
    payment_method: Option<&'a str>,
    /// The list of payment method types (for example, card) that this SetupIntent can use. If you
    /// don’t provide this, it defaults to [“card”].
    payment_method_types: Vec<&'a str>,
}

impl CreateSetupIntent<'_> {
    pub fn new(org_id: OrgId, user_id: UserId) -> Self {
        Self {
            payment_method_types: vec!["card"],
            metadata: Some(super::Metadata(hashmap! {
                "org_id".to_string() => org_id.to_string(),
                "created_by_user".to_string() => user_id.to_string(),
            })),
            ..Default::default()
        }
    }
}

impl super::StripeEndpoint for CreateSetupIntent<'_> {
    type Result = SetupIntent;

    fn method(&self) -> Method {
        Method::POST
    }

    fn path(&self) -> String {
        "setup_intents".to_string()
    }

    fn body(&self) -> Option<String> {
        serde_json::to_string(self)
            .map_err(|err| warn!("Failed to serialize {}: {}", type_name::<Self>(), err))
            .ok()
    }
}
