use crate::model::{Org, User};
use derive_more::{Deref, Display, From, FromStr};
use diesel_derive_newtype::DieselNewType;

#[derive(
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Display,
    Hash,
    PartialEq,
    Eq,
    DieselNewType,
    Deref,
    From,
    FromStr,
)]
pub struct CustomerId(String);

/// The resource representing a Stripe "Customer".
///
/// For more details see <https://stripe.com/docs/api/customers/object>
#[derive(Debug, serde::Deserialize)]
pub struct Customer {
    /// Unique identifier for the object.
    pub id: CustomerId,
    /// The customer's address.
    pub address: Option<super::address::Address>,
    /// The current balance, if any, that's stored on the customer.
    ///
    /// If negative, the customer has credit to apply to their next invoice. If positive, the
    /// customer has an amount owed that's added to their next invoice. The balance only considers
    /// amounts that Stripe hasn't successfully applied to any invoice. It doesn't reflect unpaid
    /// invoices. This balance is only taken into account after invoices finalize.
    pub balance: Option<i64>,
    /// The current funds being held by Stripe on behalf of the customer.
    ///
    /// You can apply these funds towards payment intents when the source is "cash_balance".
    /// The `settings[reconciliation_mode]` field describes if these funds apply to these payment intents manually or automatically.
    pub cash_balance: Option<CashBalance>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// Three-letter [ISO code for the currency](https://stripe.com/docs/currencies) the customer can be charged in for recurring billing purposes.
    pub currency: Option<super::currency::Currency>,
    /// ID of the default payment source for the customer.
    ///
    /// If you use payment methods created through the PaymentMethods API, see the [invoice_settings.default_payment_method](https://stripe.com/docs/api/customers/object#customer_object-invoice_settings-default_payment_method) field instead.
    pub default_source: Option<super::IdOrObject<String, PaymentSource>>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// Tracks the most recent state change on any invoice belonging to the customer.
    ///
    /// Paying an invoice or marking it uncollectible via the API will set this field to false.
    /// An automatic payment failure or passing the `invoice.due_date` will set this field to `true`.  If an invoice becomes uncollectible by [dunning](https://stripe.com/docs/billing/automatic-collection), `delinquent` doesn't reset to `false`.  If you care whether the customer has paid their most recent subscription invoice, use `subscription.status` instead.
    /// Paying or marking uncollectible any customer invoice regardless of whether it is the latest invoice for a subscription will always set this field to `false`.
    pub delinquent: Option<bool>,
    /// An arbitrary string attached to the object.
    ///
    /// Often useful for displaying to users.
    pub description: Option<String>,
    // /// Describes the current discount active on the customer, if there is one.
    // pub discount: Option<Discount>,
    /// The customer's email address.
    pub email: Option<String>,
    /// The current multi-currency balances, if any, that's stored on the customer.
    ///
    /// If positive in a currency, the customer has a credit to apply to their next invoice
    /// denominated in that currency. If negative, the customer has an amount owed that's added to
    /// their next invoice denominated in that currency. These balances don't apply to unpaid
    /// invoices. They solely track amounts that Stripe hasn't successfully applied to any invoice.
    /// Stripe only applies a balance in a specific currency to an invoice after that invoice
    /// (which is in the same currency) finalizes.
    pub invoice_credit_balance: Option<i64>,
    /// The prefix for the customer used to generate unique invoice numbers.
    pub invoice_prefix: Option<String>,
    // pub invoice_settings: Option<InvoiceSettingCustomerSetting>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object exists in test mode.
    pub livemode: Option<bool>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an object.
    ///
    /// This can be useful for storing additional information about the object in a structured format.
    pub metadata: Option<super::Metadata>,
    /// The customer's full name or business name.
    pub name: Option<String>,
    /// The suffix of the customer's next invoice number (for example, 0001).
    pub next_invoice_sequence: Option<i64>,
    /// The customer's phone number.
    pub phone: Option<String>,
    /// The customer's preferred locales (languages), ordered by preference.
    pub preferred_locales: Option<Vec<String>>,
    // /// Mailing and shipping address for the customer.
    // ///
    // /// Appears on invoices emailed to this customer.
    // pub shipping: Option<Shipping>,
    // /// The customer's payment sources, if any.
    // #[serde(default)]
    // pub sources: List<PaymentSource>,
    // /// The customer's current subscriptions, if any.
    // pub subscriptions: Option<List<Subscription>>,
    // pub tax: Option<CustomerTax>,
    // /// Describes the customer's tax exemption status, which is `none`, `exempt`, or `reverse`.
    // ///
    // /// When set to `reverse`, invoice and receipt PDFs include the following text:
    // /// **"Reverse charge"**.
    // pub tax_exempt: Option<CustomerTaxExempt>,
    // /// The customer's tax IDs.
    // pub tax_ids: Option<List<TaxId>>,
    // /// ID of the test clock that this customer belongs to.
    // pub test_clock: Option<super::IdOrObject<String, TestHelpersTestClock>>,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateCustomer<'a> {
    name: String,
    address: Option<super::address::Address>,
    email: Option<&'a str>,
    metadata: Option<super::Metadata>,
    payment_method: Option<&'a super::PaymentMethodId>,
    #[serde(rename = "invoice_settings[default_payment_method]")]
    #[serde(skip_serializing_if = "Option::is_none")]
    invoice_settings_default_payment_method: Option<&'a super::PaymentMethodId>,
    phone: Option<&'a str>,
}

impl<'a> CreateCustomer<'a> {
    pub fn new(
        org: &'a Org,
        user: &'a User,
        payment_method_id: Option<&'a super::PaymentMethodId>,
    ) -> Self {
        Self {
            name: org.name.clone(),
            address: None,
            email: Some(&user.email),
            metadata: None,
            payment_method: payment_method_id,
            invoice_settings_default_payment_method: payment_method_id,
            phone: None,
        }
    }
}

impl super::StripeEndpoint for CreateCustomer<'_> {
    type Result = Customer;

    fn method(&self) -> hyper::Method {
        hyper::Method::POST
    }

    fn path(&self) -> String {
        "customers".to_string()
    }

    fn body(&self) -> Option<&Self> {
        Some(self)
    }
}

/// The resource representing a Stripe "cash_balance".
///
/// For more details see <https://stripe.com/docs/api/cash_balance/object>
#[derive(Debug, serde::Deserialize)]
pub struct CashBalance {
    /// A hash of all cash balances available to this customer.
    ///
    /// You cannot delete a customer with any cash balances, even if the balance is 0.
    /// Amounts are represented in the
    /// [smallest currency unit](https://stripe.com/docs/currencies#zero-decimal).
    pub available: Option<i64>,
    /// The ID of the customer whose cash balance this object represents.
    pub customer: String,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: bool,
    pub settings: CashBalanceSettings,
}

#[derive(Debug, serde::Deserialize)]
pub struct CashBalanceSettings {
    /// The configuration for how funds that land in the customer cash balance are reconciled.
    pub reconciliation_mode: ReconciliationMode,
    /// A flag to indicate if reconciliation mode returned is the user's default or is specific to
    /// this customer cash balance.
    pub using_merchant_default: bool,
}

/// An enum representing the possible values of an `CustomerBalanceCustomerBalanceSettings`'s
/// `reconciliation_mode` field.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationMode {
    Automatic,
    Manual,
}

/// A PaymentSource represents a payment method _associated with a customer or charge_.
/// This value is usually returned as a subresource on another request.
///
/// Not to be confused with `Source` which represents a "generic" payment method
/// returned by the `Source::get` (which could still be a credit card, etc)
/// but is not necessarily attached to either a customer or charge.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "object", rename_all = "snake_case")]
pub enum PaymentSource {
    Card(Box<super::card::Card>),
    // Source(Source),
    // Account(Account),
    // BankAccount(BankAccount),
    #[serde(other)]
    Other,
}

#[derive(Debug, serde::Serialize)]
pub struct GetCustomer<'a> {
    customer_id: &'a CustomerId,
}

impl<'a> GetCustomer<'a> {
    pub const fn new(customer_id: &'a CustomerId) -> Self {
        Self { customer_id }
    }
}

impl super::StripeEndpoint for GetCustomer<'_> {
    type Result = Customer;

    fn method(&self) -> hyper::Method {
        hyper::Method::GET
    }

    fn path(&self) -> String {
        format!("customers/{}", self.customer_id)
    }

    fn query(&self) -> Option<&Self> {
        None
    }

    fn body(&self) -> Option<&Self> {
        None
    }
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateCustomer<'a> {
    #[serde(skip_serializing)]
    customer_id: &'a str,

    #[serde(rename = "address[city]")]
    address_city: Option<&'a str>,
    #[serde(rename = "address[country]")]
    address_country: Option<&'a str>,
    #[serde(rename = "address[line1]")]
    address_line1: Option<&'a str>,
    #[serde(rename = "address[line2]")]
    address_line2: Option<&'a str>,
    #[serde(rename = "address[postal_code]")]
    address_postal_code: Option<&'a str>,
    #[serde(rename = "address[state]")]
    address_state: Option<&'a str>,
}

impl<'a> UpdateCustomer<'a> {
    pub const fn new(
        customer_id: &'a str,
        city: Option<&'a str>,
        country: Option<&'a str>,
        line1: Option<&'a str>,
        line2: Option<&'a str>,
        postal_code: Option<&'a str>,
        state: Option<&'a str>,
    ) -> Self {
        Self {
            customer_id,
            address_city: city,
            address_country: country,
            address_line1: line1,
            address_line2: line2,
            address_postal_code: postal_code,
            address_state: state,
        }
    }
}

impl super::StripeEndpoint for UpdateCustomer<'_> {
    type Result = Customer;

    fn method(&self) -> hyper::Method {
        hyper::Method::POST
    }

    fn path(&self) -> String {
        format!("customers/{}", self.customer_id)
    }

    fn query(&self) -> Option<&Self> {
        None
    }

    fn body(&self) -> Option<&Self> {
        Some(self)
    }
}
