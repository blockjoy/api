/// The resource representing a Stripe "Plan".
///
/// For more details see <https://stripe.com/docs/api/plans/object>
#[derive(Debug, serde::Deserialize)]
pub struct Plan {
    /// Unique identifier for the object.
    pub id: String,
    /// Whether the plan can be used for new purchases.
    pub active: Option<bool>,
    /// Specifies a usage aggregation strategy for plans of `usage_type=metered`.
    ///
    /// Allowed values are `sum` for summing up all usage during a period, `last_during_period` for
    /// using the last usage record reported within a period, `last_ever` for using the last usage
    /// record ever (across period bounds) or `max` which uses the usage record with the maximum
    /// reported usage during a period.
    /// Defaults to `sum`.
    pub aggregate_usage: Option<PlanAggregateUsage>,
    /// The unit amount in cents (or local equivalent) to be charged, represented as a whole integer
    /// if possible.
    ///
    /// Only set if `billing_scheme=per_unit`.
    pub amount: Option<i64>,
    /// The unit amount in cents (or local equivalent) to be charged, represented as a decimal
    /// string with at most 12 decimal places.
    ///
    /// Only set if `billing_scheme=per_unit`.
    pub amount_decimal: Option<String>,
    /// Describes how to compute the price per period.
    ///
    /// Either `per_unit` or `tiered`.
    /// `per_unit` indicates that the fixed amount (specified in `amount`) will be charged per unit
    /// in `quantity` (for plans with `usage_type=licensed`), or per unit of total usage (for plans
    /// with `usage_type=metered`). `tiered` indicates that the unit pricing will be computed using
    /// a tiering strategy as defined using the `tiers` and `tiers_mode` attributes.
    pub billing_scheme: Option<PlanBillingScheme>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// Three-letter [ISO currency code](https://www.iso.org/iso-4217-currency-codes.html), in
    /// lowercase.
    ///
    /// Must be a [supported currency](https://stripe.com/docs/currencies).
    pub currency: Option<super::currency::Currency>,
    // Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// The frequency at which a subscription is billed.
    ///
    /// One of `day`, `week`, `month` or `year`.
    pub interval: Option<PlanInterval>,
    /// The number of intervals (specified in the `interval` attribute) between subscription
    /// billings.
    ///
    /// For example, `interval=month` and `interval_count=3` bills every 3 months.
    pub interval_count: Option<u64>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: Option<bool>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// A brief description of the plan, hidden from customers.
    pub nickname: Option<String>,
    // /// The product whose pricing this plan determines.
    // pub product: Option<super::IdOrObject<String, Product>>,
    // /// Each element represents a pricing tier.
    // ///
    // /// This parameter requires `billing_scheme` to be set to `tiered`. See also the documentation
    // /// for `billing_scheme`.
    // pub tiers: Option<Vec<PlanTier>>,
    // /// Defines if the tiering price should be `graduated` or `volume` based.
    // ///
    // /// In `volume`-based tiering, the maximum quantity within a period determines the per unit
    // /// price. In `graduated` tiering, pricing can change as the quantity grows.
    // pub tiers_mode: Option<PlanTiersMode>,
    /// Apply a transformation to the reported usage or set quantity before computing the amount
    /// billed.
    ///
    /// Cannot be combined with `tiers`.
    pub transform_usage: Option<TransformUsage>,
    /// Default number of trial days when subscribing a customer to this plan using
    /// [`trial_from_plan=true`](https://stripe.com/docs/api#create_subscription-trial_from_plan).
    pub trial_period_days: Option<u32>,
    /// Configures how the quantity per period should be determined.
    ///
    /// Can be either `metered` or `licensed`. `licensed` automatically bills the `quantity` set
    /// when adding it to a subscription. `metered` aggregates the total usage based on usage
    /// records. Defaults to `licensed`.
    pub usage_type: Option<PlanUsageType>,
}

#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAggregateUsage {
    LastDuringPeriod,
    LastEver,
    Max,
    Sum,
}

#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanBillingScheme {
    PerUnit,
    Tiered,
}

#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanInterval {
    Day,
    Month,
    Week,
    Year,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransformUsage {
    /// Divide usage by this number.
    pub divide_by: i64,

    /// After division, either round the result `up` or `down`.
    pub round: TransformUsageRound,
}

#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformUsageRound {
    Down,
    Up,
}

/// An enum representing the possible values of an `Plan`'s `usage_type` field.
#[derive(Copy, Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanUsageType {
    Licensed,
    Metered,
}
