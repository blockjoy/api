/// The resource representing a Stripe "Account".
///
/// For more details see <https://stripe.com/docs/api/accounts/object>
#[derive(Debug, serde::Deserialize)]
pub struct Account {
    /// Unique identifier for the object.
    pub id: String,
    // /// Business information about the account.
    // pub business_profile: Option<BusinessProfile>,
    // /// The business type.
    // ///
    // /// Once you create an [Account Link](https://stripe.com/docs/api/account_links) or
    // /// [Account Session](https://stripe.com/docs/api/account_sessions), this property is only
    // /// returned for Custom accounts.
    // pub business_type: Option<AccountBusinessType>,
    // pub capabilities: Option<AccountCapabilities>,
    /// Whether the account can create live charges.
    pub charges_enabled: Option<bool>,
    // pub company: Option<Company>,
    // pub controller: Option<AccountUnificationAccountController>,
    /// The account's country.
    pub country: Option<String>,
    /// Time at which the account was connected.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// Three-letter ISO currency code representing the default currency for the account.
    ///
    /// This must be a currency that
    /// [Stripe supports in the account's country](https://stripe.com/docs/payouts).
    pub default_currency: Option<super::currency::Currency>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// Whether account details have been submitted.
    ///
    /// Standard accounts cannot receive payouts before this is true.
    pub details_submitted: Option<bool>,
    /// An email address associated with the account.
    ///
    /// It's not used for authentication and Stripe doesn't market to this field without explicit
    /// approval from the platform.
    pub email: Option<String>,
    // /// External accounts (bank accounts and debit cards) currently attached to this account.
    // ///
    // /// External accounts are only returned for requests where `controller[is_controller]` is true.
    // pub external_accounts: Option<List<ExternalAccount>>,
    // pub future_requirements: Option<AccountFutureRequirements>,
    // pub individual: Option<Person>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// Whether Stripe can send payouts to this account.
    pub payouts_enabled: Option<bool>,
    // pub requirements: Option<AccountRequirements>,
    // /// Options for customizing how the account functions within Stripe.
    // pub settings: Option<AccountSettings>,
    // pub tos_acceptance: Option<TosAcceptance>,
    // /// The Stripe account type.
    // ///
    // /// Can be `standard`, `express`, or `custom`.
    // #[serde(rename = "type")]
    // pub type_: Option<AccountType>,
}
