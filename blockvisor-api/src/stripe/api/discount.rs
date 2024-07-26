/// The resource representing a Stripe "Discount".
///
/// For more details see <https://stripe.com/docs/api/discounts/object>
#[derive(Debug, serde::Deserialize)]
pub struct Discount {
    /// The ID of the discount object.
    ///
    /// Discounts cannot be fetched by ID. Use `expand[]=discounts` in API calls to expand discount
    /// IDs in an array.
    pub id: String,
    /// The Checkout session that this coupon is applied to, if it is applied to a particular
    /// session in payment mode.
    ///
    /// Will not be present for subscription mode.
    pub checkout_session: Option<String>,
    pub coupon: Coupon,
    /// The ID of the customer associated with this discount.
    pub customer: Option<super::IdOrObject<String, super::customer::Customer>>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// If the coupon has a duration of `repeating`, the date that this discount will end.
    ///
    /// If the coupon has a duration of `once` or `forever`, this attribute will be null.
    pub end: Option<super::Timestamp>,
    /// The invoice that the discount's coupon was applied to, if it was applied directly to a
    /// particular invoice.
    pub invoice: Option<String>,
    /// The invoice item `id` (or invoice line item `id` for invoice line items of
    /// type='subscription') that the discount's coupon was applied to, if it was applied directly
    /// to a particular invoice item or invoice line item.
    pub invoice_item: Option<String>,
    // /// The promotion code applied to create this discount.
    // pub promotion_code: Option<super::IdOrObject<String, PromotionCode>>,
    /// Date that the coupon was applied.
    pub start: super::Timestamp,
    /// The subscription that this coupon is applied to, if it is applied to a particular
    /// subscription.
    pub subscription: Option<String>,
}

/// The resource representing a Stripe "Coupon".
///
/// For more details see <https://stripe.com/docs/api/coupons/object>
#[derive(Debug, serde::Deserialize)]
pub struct Coupon {
    /// Unique identifier for the object.
    pub id: String,
    /// Amount (in the `currency` specified) that will be taken off the subtotal of any invoices for
    /// this customer.
    pub amount_off: Option<i64>,
    // pub applies_to: Option<CouponAppliesTo>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// If `amount_off` has been set, the three-letter [ISO code for the currency]
    /// (https://stripe.com/docs/currencies) of the amount to take off.
    pub currency: Option<super::currency::Currency>,
    // /// Coupons defined in each available currency option.
    // ///
    // /// Each key must be a three-letter [ISO currency code]
    // /// (https://www.iso.org/iso-4217-currency-codes.html) and a [supported currency]
    // /// (https://stripe.com/docs/currencies).
    // pub currency_options: Option<CurrencyMap<CouponCurrencyOption>>,
    // Always true for a deleted object
    pub deleted: bool,
    /// One of `forever`, `once`, and `repeating`.
    ///
    /// Describes how long a customer who applies this coupon will get the discount.
    pub duration: Option<CouponDuration>,
    /// If `duration` is `repeating`, the number of months the coupon applies.
    ///
    /// Null if coupon `duration` is `forever` or `once`.
    pub duration_in_months: Option<i64>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: Option<bool>,
    /// Maximum number of times this coupon can be redeemed, in total, across all customers, before
    /// it is no longer valid.
    pub max_redemptions: Option<i64>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// Name of the coupon displayed to customers on for instance invoices or receipts.
    pub name: Option<String>,
    /// Percent that will be taken off the subtotal of any invoices for this customer for the
    /// duration of the coupon.
    ///
    /// For example, a coupon with percent_off of 50 will make a $ (or local equivalent)100 invoice
    /// $ (or local equivalent)50 instead.
    pub percent_off: Option<f64>,
    /// Date after which the coupon can no longer be redeemed.
    pub redeem_by: Option<super::Timestamp>,
    /// Number of times this coupon has been applied to a customer.
    pub times_redeemed: Option<i64>,
    /// Taking account of the above properties, whether this coupon can still be applied to a
    /// customer.
    pub valid: Option<bool>,
}

/// An enum representing the possible values of an `Coupon`'s `duration` field.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CouponDuration {
    Forever,
    Once,
    Repeating,
}
