/// The resource representing a Stripe "Card".
///
/// For more details see <https://stripe.com/docs/api/cards/object>
#[derive(Debug, serde::Deserialize)]
pub struct Card {
    /// Unique identifier for the object.
    pub id: String,
    /// The account this card belongs to.
    ///
    /// This attribute will not be in the card object if the card belongs to a customer or recipient
    /// instead.
    pub account: Option<super::IdOrObject<String, super::account::Account>>,
    /// City/District/Suburb/Town/Village.
    pub address_city: Option<String>,
    /// Billing address country, if provided when creating card.
    pub address_country: Option<String>,
    /// Address line 1 (Street address/PO Box/Company name).
    pub address_line1: Option<String>,
    /// If `address_line1` was provided, results of the check: `pass`, `fail`, `unavailable`, or
    /// `unchecked`.
    pub address_line1_check: Option<String>,
    /// Address line 2 (Apartment/Suite/Unit/Building).
    pub address_line2: Option<String>,
    /// State/County/Province/Region.
    pub address_state: Option<String>,
    /// ZIP or postal code.
    pub address_zip: Option<String>,
    /// If `address_zip` was provided, results of the check: `pass`, `fail`, `unavailable`, or
    /// `unchecked`.
    pub address_zip_check: Option<String>,
    // /// A set of available payout methods for this card.
    // ///
    // /// Only values from this set should be passed as the `method` when creating a payout.
    // pub available_payout_methods: Option<Vec<CardAvailablePayoutMethods>>,
    /// Card brand.
    ///
    /// Can be `American Express`, `Diners Club`, `Discover`, `Eftpos Australia`, `JCB`,
    /// `MasterCard`, `UnionPay`, `Visa`, or `Unknown`.
    pub brand: Option<String>,
    /// Two-letter ISO code representing the country of the card.
    ///
    /// You could use this attribute to get a sense of the international breakdown of cards you've
    /// collected.
    pub country: Option<String>,
    /// Three-letter [ISO code for currency](https://stripe.com/docs/payouts).
    ///
    /// Only applicable on accounts (not customers or recipients). The card can be used as a
    /// transfer destination for funds in this currency.
    pub currency: Option<super::currency::Currency>,
    /// The customer that this card belongs to.
    ///
    /// This attribute will not be in the card object if the card belongs to an account or recipient
    /// instead.
    pub customer: Option<super::IdOrObject<String, super::customer::Customer>>,
    /// If a CVC was provided, results of the check: `pass`, `fail`, `unavailable`, or `unchecked`.
    ///
    /// A result of unchecked indicates that CVC was provided but hasn't been checked yet. Checks
    /// are typically performed when attaching a card to a Customer object, or when creating a
    /// charge. For more details, see [Check if a card is valid without a charge]
    /// (https://support.stripe.com/questions/check-if-a-card-is-valid-without-a-charge).
    pub cvc_check: Option<String>,
    /// Whether this card is the default external account for its currency.
    pub default_for_currency: Option<bool>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// A high-level description of the type of cards issued in this range.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub description: Option<String>,
    /// (For tokenized numbers only.) The last four digits of the device account number.
    pub dynamic_last4: Option<String>,
    /// Two-digit number representing the card's expiration month.
    pub exp_month: Option<i64>,
    /// Four-digit number representing the card's expiration year.
    pub exp_year: Option<i64>,
    /// Uniquely identifies this particular card number.
    ///
    /// You can use this attribute to check whether two customers who’ve signed up with you are
    /// using the same card number, for example. For payment methods that tokenize card information
    /// (Apple Pay, Google Pay), the tokenized number might be provided instead of the underlying
    /// card number.  *As of May 1, 2021, card fingerprint in India for Connect changed to allow two
    /// fingerprints for the same card---one for India and one for the rest of the world.*.
    pub fingerprint: Option<String>,
    /// Card funding type.
    ///
    /// Can be `credit`, `debit`, `prepaid`, or `unknown`.
    pub funding: Option<String>,
    /// Issuer identification number of the card.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub iin: Option<String>,
    /// The name of the card's issuing bank.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub issuer: Option<String>,
    /// The last four digits of the card.
    pub last4: Option<String>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// Cardholder name.
    pub name: Option<String>,
    /// For external accounts that are cards, possible values are `new` and `errored`.
    ///
    /// If a payout fails, the status is set to `errored` and [scheduled payouts]
    /// (https://stripe.com/docs/payouts#payout-schedule) are stopped until account details are
    /// updated.
    pub status: Option<String>,
    /// If the card number is tokenized, this is the method that was used.
    ///
    /// Can be `android_pay` (includes Google Pay), `apple_pay`, `masterpass`, `visa_checkout`, or
    /// null.
    pub tokenization_method: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CardDetails {
    /// Card brand.
    ///
    /// Can be `amex`, `diners`, `discover`, `eftpos_au`, `jcb`, `mastercard`, `unionpay`, `visa`,
    /// or `unknown`.
    pub brand: String,
    // /// Checks on Card address and CVC if provided.
    // pub checks: Option<PaymentMethodCardChecks>,
    /// Two-letter ISO code representing the country of the card.
    ///
    /// You could use this attribute to get a sense of the international breakdown of cards you've
    /// collected.
    pub country: Option<String>,
    /// A high-level description of the type of cards issued in this range.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub description: Option<String>,
    /// Two-digit number representing the card's expiration month.
    pub exp_month: i64,
    /// Four-digit number representing the card's expiration year.
    pub exp_year: i64,
    /// Uniquely identifies this particular card number.
    ///
    /// You can use this attribute to check whether two customers who’ve signed up with you are
    /// using the same card number, for example. For payment methods that tokenize card information
    /// (Apple Pay, Google Pay), the tokenized number might be provided instead of the underlying
    /// card number.  *As of May 1, 2021, card fingerprint in India for Connect changed to allow two
    /// fingerprints for the same card---one for India and one for the rest of the world.*.
    pub fingerprint: Option<String>,
    /// Card funding type.
    ///
    /// Can be `credit`, `debit`, `prepaid`, or `unknown`.
    pub funding: String,
    /// Issuer identification number of the card.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub iin: Option<String>,
    /// The name of the card's issuing bank.
    ///
    /// (For internal use only and not typically available in standard API requests.).
    pub issuer: Option<String>,
    /// The last four digits of the card.
    pub last4: String,
    // /// Contains information about card networks that can be used to process the payment.
    // pub networks: Option<Networks>,
    // /// Contains details on how this Card may be used for 3D Secure authentication.
    // pub three_d_secure_usage: Option<ThreeDSecureUsage>,
    // /// If this Card is part of a card wallet, this contains the details of the card wallet.
    // pub wallet: Option<WalletDetails>,
}
