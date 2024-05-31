use std::collections::HashMap;

#[derive(Debug, serde::Deserialize)]
pub struct Event {
    /// Unique identifier for the object.
    pub id: EventId,
    /// The connected account that originates the event.
    pub account: Option<String>,
    /// The Stripe API version used to render `data`.
    ///
    /// This property is populated only for events on or after October 31, 2014.
    pub api_version: Option<String>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: super::Timestamp,
    pub data: NotificationEventData,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: bool,
    /// Number of webhooks that haven't been successfully delivered (for example, to return a 20x
    /// response) to the URLs you specify.
    pub pending_webhooks: i64,
    // /// Information on the API request that triggers the event.
    // pub request: Option<NotificationEventRequest>,
    /// Description of the event (for example, `invoice.created` or `charge.refunded`).
    #[serde(rename = "type")]
    pub type_: EventType,
}

#[derive(Debug, Default, serde::Deserialize)]
pub enum EventType {
    #[serde(rename = "setup_intent.canceled")]
    SetupIntentCanceled,
    #[serde(rename = "setup_intent.created")]
    SetupIntentCreated,
    #[serde(rename = "setup_intent.requires_action")]
    SetupIntentRequiresAction,
    #[serde(rename = "setup_intent.setup_failed")]
    SetupIntentSetupFailed,
    #[serde(rename = "setup_intent.succeeded")]
    SetupIntentSucceeded,
    #[serde(other)]
    #[default]
    Other,
}

#[derive(Debug, serde::Deserialize)]
pub struct NotificationEventData {
    pub object: EventObject,
    pub previous_attributes: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "object", rename_all = "snake_case")]
pub enum EventObject {
    SetupIntent(SetupIntent),
    #[serde(other)]
    Other,
}

/// The resource representing a Stripe "SetupIntent".
///
/// For more details see <https://stripe.com/docs/api/setup_intents/object>
#[derive(Debug, serde::Deserialize)]
pub struct SetupIntent {
    /// Unique identifier for the object.
    pub id: SetupIntentId,
    /// ID of the Connect application that created the SetupIntent.
    pub application: Option<super::IdOrObject<String, Application>>,
    /// If present, the SetupIntent's payment method will be attached to the in-context Stripe
    /// Account.
    ///
    /// It can only be used for this Stripe Account’s own money movement flows like
    /// InboundTransfer and OutboundTransfers.
    ///
    /// It cannot be set to true when setting up a PaymentMethod for a Customer, and defaults to
    /// false when attaching a PaymentMethod to a Customer.
    pub attach_to_self: Option<bool>,
    /// Settings for dynamic payment methods compatible with this Setup Intent.
    pub automatic_payment_methods: Option<AutomaticPaymentMethods>,
    /// Reason for cancellation of this SetupIntent, one of `abandoned`, `requested_by_customer`,
    /// or `duplicate`.
    pub cancellation_reason: Option<CancellationReason>,
    /// The client secret of this SetupIntent.
    ///
    /// Used for client-side retrieval using a publishable key.  The client secret can be used to
    /// complete payment setup from your frontend.
    /// It should not be stored, logged, or exposed to anyone other than the customer.
    /// Make sure that you have TLS enabled on any page that includes the client secret.
    pub client_secret: Option<String>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: super::Timestamp,
    /// ID of the Customer this SetupIntent belongs to, if one exists.
    ///
    /// If present, the SetupIntent's payment method will be attached to the Customer on successful
    /// setup.
    ///
    /// Payment methods attached to other Customers cannot be used with this SetupIntent.
    pub customer: Option<super::IdOrObject<String, super::customer::Customer>>,
    /// An arbitrary string attached to the object.
    ///
    /// Often useful for displaying to users.
    pub description: Option<String>,
    /// Indicates the directions of money movement for which this payment method is intended to be
    /// used.
    ///
    /// Include `inbound` if you intend to use the payment method as the origin to pull funds from.
    ///
    /// Include `outbound` if you intend to use the payment method as the destination to send funds
    /// to. You can include both if you intend to use the payment method for both purposes.
    pub flow_directions: Option<Vec<SetupIntentFlowDirections>>,
    // /// The error encountered in the previous SetupIntent confirmation.
    // pub last_setup_error: Option<Box<ApiErrors>>,
    // /// The most recent SetupAttempt for this SetupIntent.
    // pub latest_attempt: Option<super::IdOrObject<String, SetupAttempt>>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: bool,
    // /// ID of the multi use Mandate generated by the SetupIntent.
    // pub mandate: Option<super::IdOrObject<String, Mandate>>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    // /// If present, this property tells you what actions you need to take in order for your customer
    // /// to continue payment setup.
    // pub next_action: Option<SetupIntentNextAction>,
    // /// The account (if any) for which the setup is intended.
    // pub on_behalf_of: Option<super::IdOrObject<String, Account>>,
    /// ID of the payment method used with this SetupIntent.
    pub payment_method: super::PaymentMethodId,
    // /// Information about the payment method configuration used for this Setup Intent.
    // payment_method_configuration_details:
    //     Option<PaymentMethodConfigBizPaymentMethodConfigurationDetails>,
    // /// Payment method-specific configuration for this SetupIntent.
    // pub payment_method_options: Option<SetupIntentPaymentMethodOptions>,
    // /// The list of payment method types (e.g.
    // ///
    // /// card) that this SetupIntent is allowed to set up.
    // pub payment_method_types: Vec<String>,
    // /// ID of the single_use Mandate generated by the SetupIntent.
    // pub single_use_mandate: Option<super::IdOrObject<String, Mandate>>,
    // /// [Status](https://stripe.com/docs/payments/intents#intent-statuses) of this SetupIntent, one
    // /// of `requires_payment_method`, `requires_confirmation`, `requires_action`, `processing`,
    // /// `canceled`, or `succeeded`.
    // pub status: SetupIntentStatus,
    // /// Indicates how the payment method is intended to be used in the future.
    // ///
    // /// Use `on_session` if you intend to only reuse the payment method when the customer is in your
    // /// checkout flow.
    // ///
    // /// Use `off_session` if your customer may or may not be in your checkout flow.
    // /// If not provided, this value defaults to `off_session`.
    // pub usage: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct EventId(pub String);

#[derive(Debug, serde::Deserialize)]
pub struct SetupIntentId(pub String);

/// An enum representing the possible values of an `SetupIntent`'s `flow_directions` field.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupIntentFlowDirections {
    Inbound,
    Outbound,
}

/// The resource representing a Stripe "Application".
#[derive(Debug, serde::Deserialize)]
pub struct Application {
    /// Unique identifier for the object.
    pub id: String,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// The name of the application.
    pub name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AutomaticPaymentMethods {
    /// Controls whether this SetupIntent will accept redirect-based payment methods.
    ///
    /// Redirect-based payment methods may require your customer to be redirected to a payment
    /// method's app or site for authentication or additional steps.
    ///
    /// To [confirm](https://stripe.com/docs/api/setup_intents/confirm) this SetupIntent, you may
    /// be required to provide a `return_url` to redirect customers back to your site after they
    /// authenticate or complete the setup.
    pub allow_redirects: Option<AllowRedirects>,
    /// Automatically calculates compatible payment methods.
    pub enabled: Option<bool>,
}

/// An enum representing the possible values of an `AutomaticPaymentMethods`'s `allow_redirects`
/// field.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowRedirects {
    Always,
    Never,
}

/// An enum representing the possible values of an `SetupIntent`'s `cancellation_reason` field.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    Abandoned,
    Duplicate,
    RequestedByCustomer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_parse_example_event() {
        let test_event = r#"{
          "id": "evt_1PJF2LB5ce1jJsfTPpe5jYGO",
          "object": "event",
          "account": "acct_1KfoP7B5ce1jJsfT",
          "api_version": "2020-08-27",
          "created": 1716383816,
          "data": {
            "object": {
              "id": "seti_1PJF2JB5ce1jJsfTFGK2BsA2",
              "object": "setup_intent",
              "application": null,
              "automatic_payment_methods": null,
              "cancellation_reason": null,
              "client_secret": null,
              "created": 1716383815,
              "customer": null,
              "description": null,
              "flow_directions": null,
              "last_setup_error": null,
              "latest_attempt": "setatt_1PJF2KB5ce1jJsfT7cnqvcdJ",
              "livemode": false,
              "mandate": null,
              "metadata": {
              },
              "next_action": null,
              "on_behalf_of": null,
              "payment_method": "pm_1PJF2KB5ce1jJsfTwnjCtwQ7",
              "payment_method_configuration_details": null,
              "payment_method_options": {
                "card": {
                  "mandate_options": null,
                  "network": null,
                  "request_three_d_secure": "automatic"
                }
              },
              "payment_method_types": [
                "card"
              ],
              "single_use_mandate": null,
              "status": "succeeded",
              "usage": "off_session"
            }
          },
          "livemode": false,
          "pending_webhooks": 1,
          "request": {
            "id": "req_pOKcxaRzqqu5mf",
            "idempotency_key": "7d47d88d-708b-41bd-8f7d-88c8c472173d"
          },
          "type": "setup_intent.succeeded"
        }"#;
        let _: Event = serde_json::from_str(test_event).unwrap();
    }
}
