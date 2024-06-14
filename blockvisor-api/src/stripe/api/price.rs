#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PriceId(pub String);

/// The resource representing a Stripe "Price".
///
/// For more details see <https://stripe.com/docs/api/prices/object>
#[derive(Debug, serde::Deserialize)]
pub struct Price {
    /// Unique identifier for the object.
    pub id: PriceId,
    /// Whether the price can be used for new purchases.
    pub active: Option<bool>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// Three-letter [ISO currency code](https://www.iso.org/iso-4217-currency-codes.html),
    /// in lowercase.
    ///
    /// Must be a [supported currency](https://stripe.com/docs/currencies).
    pub currency: Option<super::currency::Currency>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: Option<bool>,
    /// A lookup key used to retrieve prices dynamically from a static string.
    ///
    /// This may be up to 200 characters.
    pub lookup_key: Option<String>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// A brief description of the price, hidden from customers.
    pub nickname: Option<String>,
    /// The unit amount in cents (or local equivalent) to be charged, represented as a whole integer
    /// if possible.
    ///
    /// Only set if `billing_scheme=per_unit`.
    pub unit_amount: Option<i64>,
    /// The unit amount in cents (or local equivalent) to be charged, represented as a decimal
    /// string with at most 12 decimal places.
    ///
    /// Only set if `billing_scheme=per_unit`.
    pub unit_amount_decimal: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchPrice {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u64>,
}

impl SearchPrice {
    pub fn new(sku: &str) -> Self {
        Self {
            query: format!("active:'true' AND metadata['sku']:'{sku}'"),
            limit: Some(2),
            page: None,
        }
    }
}

impl super::StripeEndpoint for SearchPrice {
    type Result = super::ListResponse<Price>;

    fn method(&self) -> reqwest::Method {
        reqwest::Method::GET
    }

    fn path(&self) -> String {
        "prices/search".to_string()
    }

    fn query(&self) -> Option<&Self> {
        Some(self)
    }
}
