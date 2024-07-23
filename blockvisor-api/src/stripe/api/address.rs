use crate::grpc::common;

/// The resource representing a Stripe "Address".
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Address {
    /// City, district, suburb, town, or village.
    pub city: Option<String>,
    /// Two-letter country code ([ISO 3166-1 alpha-2]
    /// (https://en.wikipedia.org/wiki/ISO_3166-1_alpha-2)).
    pub country: Option<String>,
    /// Address line 1 (e.g., street, PO Box, or company name).
    pub line1: Option<String>,
    /// Address line 2 (e.g., apartment, suite, unit, or building).
    pub line2: Option<String>,
    /// ZIP or postal code.
    pub postal_code: Option<String>,
    /// State, county, province, or region.
    pub state: Option<String>,
}

impl From<common::Address> for Address {
    fn from(value: common::Address) -> Self {
        Self {
            city: value.city,
            country: value.country,
            line1: value.line1,
            line2: value.line2,
            postal_code: value.postal_code,
            state: value.state,
        }
    }
}

impl From<Address> for common::Address {
    fn from(value: Address) -> Self {
        Self {
            city: value.city,
            country: value.country,
            line1: value.line1,
            line2: value.line2,
            postal_code: value.postal_code,
            state: value.state,
        }
    }
}
