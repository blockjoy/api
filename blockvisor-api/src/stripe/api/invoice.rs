use std::collections::HashMap;

use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::grpc::api;
use crate::util::NanosUtc;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Currency error: {0}
    Currency(#[from] super::currency::Error),
    /// Discount error: {0}
    Discount(#[from] super::discount::Error),
}

#[derive(Debug, Deserialize)]
pub struct Invoice {
    /// Unique identifier for the object.
    ///
    /// This property is always present unless the invoice is an upcoming invoice. See
    /// [Retrieve an upcoming invoice](https://stripe.com/docs/api/invoices/upcoming) for more
    /// details.
    pub id: Option<String>,
    /// The country of the business associated with this invoice, most often the business creating
    /// the invoice.
    pub account_country: Option<String>,
    /// The public name of the business associated with this invoice, most often the business
    /// creating the invoice.
    pub account_name: Option<String>,
    /// The account tax IDs associated with the invoice.
    ///
    /// Only editable when the invoice is a draft.
    pub account_tax_ids: Option<Vec<String>>,
    /// Final amount due at this time for this invoice.
    ///
    /// If the invoice's total is smaller than the minimum charge amount, for example, or if there
    /// is account credit that can be applied to the invoice, the `amount_due` may be 0. If there is
    /// a positive `starting_balance` for the invoice (the customer owes money), the `amount_due`
    /// will also take that into account. The charge that gets generated for the invoice will be for
    ///  the amount specified in `amount_due`.
    pub amount_due: Option<i64>,
    /// The amount, in cents (or local equivalent), that was paid.
    pub amount_paid: Option<i64>,
    /// The difference between amount_due and amount_paid, in cents (or local equivalent).
    pub amount_remaining: Option<i64>,
    /// This is the sum of all the shipping amounts.
    pub amount_shipping: Option<i64>,
    // /// ID of the Connect Application that created the invoice.
    // pub application: Option<super::IdOrObject<String, Application>>,
    /// The fee in cents (or local equivalent) that will be applied to the invoice and transferred
    /// to the application owner's Stripe account when the invoice is paid.
    pub application_fee_amount: Option<i64>,
    /// Number of payment attempts made for this invoice, from the perspective of the payment retry
    /// schedule.
    ///
    /// Any payment attempt counts as the first attempt, and subsequently only automatic retries
    /// increment the attempt count. In other words, manual payment attempts after the first attempt do not affect the retry schedule.
    pub attempt_count: Option<u64>,
    /// Whether an attempt has been made to pay the invoice.
    ///
    /// An invoice is not attempted until 1 hour after the `invoice.created` webhook, for example,
    /// so you might not want to display that invoice as unpaid to your users.
    pub attempted: Option<bool>,
    /// Controls whether Stripe performs
    /// [automatic collection](https://stripe.com/docs/invoicing/integration/automatic-advancement-collection) of the invoice.
    ///
    /// If `false`, the invoice's state doesn't automatically advance without an explicit action.
    pub auto_advance: Option<bool>,
    // pub automatic_tax: Option<AutomaticTax>,
    /// Indicates the reason why the invoice was created.
    ///
    /// * `manual`: Unrelated to a subscription, for example, created via the invoice editor.
    /// * `subscription`: No longer in use.
    ///
    /// Applies to subscriptions from before May 2018 where no distinction was made between updates,
    /// cycles, and thresholds. * `subscription_create`: A new subscription was created.
    /// * `subscription_cycle`: A subscription advanced into a new period.
    /// * `subscription_threshold`: A subscription reached a billing threshold.
    /// * `subscription_update`: A subscription was updated.
    /// * `upcoming`: Reserved for simulated invoices, per the upcoming invoice endpoint.
    pub billing_reason: Option<InvoiceBillingReason>,
    // /// ID of the latest charge generated for this invoice, if any.
    // pub charge: Option<super::IdOrObject<String, Charge>>,
    /// Either `charge_automatically`, or `send_invoice`.
    ///
    /// When charging automatically, Stripe will attempt to pay this invoice using the default
    /// source attached to the customer. When sending an invoice, Stripe will email this invoice to
    /// the customer with payment instructions.
    pub collection_method: Option<CollectionMethod>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: Option<super::Timestamp>,
    /// Three-letter [ISO currency code](https://www.iso.org/iso-4217-currency-codes.html), in
    /// lowercase.
    ///
    /// Must be a [supported currency](https://stripe.com/docs/currencies).
    pub currency: Option<super::currency::Currency>,
    // /// Custom fields displayed on the invoice.
    // pub custom_fields: Option<Vec<InvoiceSettingCustomField>>,
    /// The ID of the customer who will be billed.
    pub customer: Option<super::IdOrObject<String, super::customer::Customer>>,
    /// The customer's address.
    ///
    /// Until the invoice is finalized, this field will equal `customer.address`. Once the invoice
    /// is finalized, this field will no longer be updated.
    pub customer_address: Option<super::address::Address>,
    /// The customer's email.
    ///
    /// Until the invoice is finalized, this field will equal `customer.email`. Once the invoice is
    /// finalized, this field will no longer be updated.
    pub customer_email: Option<String>,
    /// The customer's name.
    ///
    /// Until the invoice is finalized, this field will equal `customer.name`. Once the invoice is
    /// finalized, this field will no longer be updated.
    pub customer_name: Option<String>,
    /// The customer's phone number.
    ///
    /// Until the invoice is finalized, this field will equal `customer.phone`. Once the invoice is
    /// finalized, this field will no longer be updated.
    pub customer_phone: Option<String>,
    // /// The customer's shipping information.
    // ///
    // /// Until the invoice is finalized, this field will equal `customer.shipping`. Once the invoice
    // /// is finalized, this field will no longer be updated.
    // pub customer_shipping: Option<Shipping>,
    /// The customer's tax exempt status.
    ///
    /// Until the invoice is finalized, this field will equal `customer.tax_exempt`. Once the
    /// invoice is finalized, this field will no longer be updated.
    pub customer_tax_exempt: Option<InvoiceCustomerTaxExempt>,
    /// The customer's tax IDs.
    ///
    /// Until the invoice is finalized, this field will contain the same tax IDs as
    /// `customer.tax_ids`. Once the invoice is finalized, this field will no longer be updated.
    pub customer_tax_ids: Option<Vec<InvoicesResourceInvoiceTaxId>>,
    /// ID of the default payment method for the invoice.
    ///
    /// It must belong to the customer associated with the invoice. If not set, defaults to the
    /// subscription's default payment method, if any, or to the default payment method in the
    /// customer's invoice settings.
    pub default_payment_method:
        Option<super::IdOrObject<String, super::payment_method::PaymentMethod>>,
    // /// ID of the default payment source for the invoice.
    // ///
    // /// It must belong to the customer associated with the invoice and be in a chargeable state. If
    // /// not set, defaults to the subscription's default source, if any, or to the customer's default
    // /// source.
    // pub default_source: Option<super::IdOrObject<String, PaymentSource>>,
    // /// The tax rates applied to this invoice, if any.
    // pub default_tax_rates: Option<Vec<TaxRate>>,
    /// Always true for a deleted object
    #[serde(default)]
    pub deleted: bool,
    /// An arbitrary string attached to the object.
    ///
    /// Often useful for displaying to users. Referenced as 'memo' in the Dashboard.
    pub description: Option<String>,
    /// Describes the current discount applied to this invoice, if there is one.
    ///
    /// Not populated if there are multiple discounts.
    pub discount: Option<super::discount::Discount>,
    /// The discounts applied to the invoice.
    ///
    /// Line item discounts are applied before invoice discounts. Use `expand[]=discounts` to expand
    /// each discount.
    pub discounts: Option<Vec<super::IdOrObject<String, super::discount::Discount>>>,
    /// The date on which payment for this invoice is due.
    ///
    /// This value will be `null` for invoices where `collection_method=charge_automatically`.
    pub due_date: Option<super::Timestamp>,
    /// The date when this invoice is in effect.
    ///
    /// Same as `finalized_at` unless overwritten. When defined, this value replaces the
    /// system-generated 'Date of issue' printed on the invoice PDF and receipt.
    pub effective_at: Option<super::Timestamp>,
    /// Ending customer balance after the invoice is finalized.
    ///
    /// Invoices are finalized approximately an hour after successful webhook delivery or when
    /// payment collection is attempted for the invoice. If the invoice has not been finalized yet,
    /// this will be null.
    pub ending_balance: Option<i64>,
    /// Footer displayed on the invoice.
    pub footer: Option<String>,
    /// Details of the invoice that was cloned.
    ///
    /// See the [revision documentation](https://stripe.com/docs/invoicing/invoice-revisions) for
    /// more details.
    pub from_invoice: Option<InvoicesFromInvoice>,
    /// The URL for the hosted invoice page, which allows customers to view and pay an invoice.
    ///
    /// If the invoice has not been finalized yet, this will be null.
    pub hosted_invoice_url: Option<String>,
    /// The link to download the PDF for the invoice.
    ///
    /// If the invoice has not been finalized yet, this will be null.
    pub invoice_pdf: Option<String>,
    // pub issuer: Option<ConnectAccountReference>,
    // /// The error encountered during the previous attempt to finalize the invoice.
    // ///
    // /// This field is cleared when the invoice is successfully finalized.
    // pub last_finalization_error: Option<Box<ApiErrors>>,
    /// The ID of the most recent non-draft revision of this invoice.
    pub latest_revision: Option<super::IdOrObject<String, Box<Invoice>>>,
    /// The individual line items that make up the invoice.
    ///
    /// `lines` is sorted as follows: (1) pending invoice items (including prorations) in reverse
    /// chronological order, (2) subscription items in reverse chronological order, and (3) invoice
    /// items added after invoice creation in chronological order.
    pub lines: Option<super::ListResponse<InvoiceLineItem>>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: Option<bool>,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format.
    pub metadata: Option<super::Metadata>,
    /// The time at which payment will next be attempted.
    ///
    /// This value will be `null` for invoices where `collection_method=send_invoice`.
    pub next_payment_attempt: Option<super::Timestamp>,
    /// A unique, identifying string that appears on emails sent to the customer for this invoice.
    ///
    /// This starts with the customer's unique invoice_prefix if it is specified.
    pub number: Option<String>,
    /// The account (if any) for which the funds of the invoice payment are intended.
    ///
    /// If set, the invoice will be presented with the branding and support information of the
    /// specified account. See the
    /// [Invoices with Connect](https://stripe.com/docs/billing/invoices/connect) documentation for
    /// details.
    pub on_behalf_of: Option<super::IdOrObject<String, super::account::Account>>,
    /// Whether payment was successfully collected for this invoice.
    ///
    /// An invoice can be paid (most commonly) with a charge or with credit from the customer's
    /// account balance.
    pub paid: Option<bool>,
    /// Returns true if the invoice was manually marked paid, returns false if the invoice hasn't
    /// been paid yet or was paid on Stripe.
    pub paid_out_of_band: Option<bool>,
    // /// The PaymentIntent associated with this invoice.
    // ///
    // /// The PaymentIntent is generated when the invoice is finalized, and can then be used to pay
    // /// the invoice.
    // /// Note that voiding an invoice will cancel the PaymentIntent.
    // pub payment_intent: Option<super::IdOrObject<String, PaymentIntent>>,
    // pub payment_settings: Option<InvoicesPaymentSettings>,
    /// End of the usage period during which invoice items were added to this invoice.
    pub period_end: Option<super::Timestamp>,
    /// Start of the usage period during which invoice items were added to this invoice.
    pub period_start: Option<super::Timestamp>,
    /// Total amount of all post-payment credit notes issued for this invoice.
    pub post_payment_credit_notes_amount: Option<i64>,
    /// Total amount of all pre-payment credit notes issued for this invoice.
    pub pre_payment_credit_notes_amount: Option<i64>,
    // /// The quote this invoice was generated from.
    // pub quote: Option<super::IdOrObject<String, Quote>>,
    /// This is the transaction number that appears on email receipts sent for this invoice.
    pub receipt_number: Option<String>,
    // /// The rendering-related settings that control how the invoice is displayed on customer-facing
    // /// surfaces such as PDF and Hosted Invoice Page.
    // pub rendering: Option<InvoicesInvoiceRendering>,
    // /// This is a legacy field that will be removed soon.
    // ///
    // /// For details about `rendering_options`, refer to `rendering` instead.
    // /// Options for invoice PDF rendering.
    // pub rendering_options: Option<InvoiceSettingRenderingOptions>,
    // /// The details of the cost of shipping, including the ShippingRate applied on the invoice.
    // pub shipping_cost: Option<InvoicesShippingCost>,
    // /// Shipping details for the invoice.
    // ///
    // /// The Invoice PDF will use the `shipping_details` value if it is set, otherwise the PDF will
    // /// render the shipping address from the customer.
    // pub shipping_details: Option<Shipping>,
    /// Starting customer balance before the invoice is finalized.
    ///
    /// If the invoice has not been finalized yet, this will be the current customer balance.
    /// For revision invoices, this also includes any customer balance that was applied to the
    /// original invoice.
    pub starting_balance: Option<i64>,
    /// Extra information about an invoice for the customer's credit card statement.
    pub statement_descriptor: Option<String>,
    /// The status of the invoice, one of `draft`, `open`, `paid`, `uncollectible`, or `void`.
    ///
    /// [Learn more](https://stripe.com/docs/billing/invoices/workflow#workflow-overview).
    pub status: Option<InvoiceStatus>,
    // pub status_transitions: Option<InvoicesStatusTransitions>,
    // /// The subscription that this invoice was prepared for, if any.
    // pub subscription: Option<super::IdOrObject<String, Subscription>>,
    // /// Details about the subscription that created this invoice.
    // pub subscription_details: Option<SubscriptionDetailsData>,
    /// Only set for upcoming invoices that preview prorations.
    ///
    /// The time used to calculate prorations.
    pub subscription_proration_date: Option<super::Timestamp>,
    /// Total of all subscriptions, invoice items, and prorations on the invoice before any invoice
    /// level discount or exclusive tax is applied.
    ///
    /// Item discounts are already incorporated.
    pub subtotal: Option<i64>,
    /// The integer amount in cents (or local equivalent) representing the subtotal of the invoice
    /// before any invoice level discount or tax is applied.
    ///
    /// Item discounts are already incorporated.
    pub subtotal_excluding_tax: Option<i64>,
    /// The amount of tax on this invoice.
    ///
    /// This is the sum of all the tax amounts on this invoice.
    pub tax: Option<i64>,
    // /// ID of the test clock this invoice belongs to.
    // pub test_clock: Option<super::IdOrObject<String, TestHelpersTestClock>>,
    // pub threshold_reason: Option<InvoiceThresholdReason>,
    /// Total after discounts and taxes.
    pub total: Option<i64>,
    // /// The aggregate amounts calculated per discount across all line items.
    pub total_discount_amounts: Option<Vec<super::discount::DiscountAmount>>,
    /// The integer amount in cents (or local equivalent) representing the total amount of the
    /// invoice including all discounts but excluding all tax.
    pub total_excluding_tax: Option<i64>,
    // /// The aggregate amounts calculated per tax rate for all line items.
    // pub total_tax_amounts: Option<Vec<TaxAmount>>,
    // /// The account (if any) the payment will be attributed to for tax reporting, and where funds
    // /// from the payment will be transferred to for the invoice.
    // pub transfer_data: Option<InvoiceTransferData>,
    /// Invoices are automatically paid or sent 1 hour after webhooks are delivered, or until all
    /// webhook delivery attempts have
    /// [been exhausted](https://stripe.com/docs/billing/webhooks#understand).
    ///
    /// This field tracks the time when webhooks for this invoice were successfully delivered. If
    /// the invoice had no webhooks to deliver, this will be set while the invoice is being created.
    pub webhooks_delivered_at: Option<super::Timestamp>,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceBillingReason {
    AutomaticPendingInvoiceItemInvoice,
    Manual,
    QuoteAccept,
    Subscription,
    SubscriptionCreate,
    SubscriptionCycle,
    SubscriptionThreshold,
    SubscriptionUpdate,
    Upcoming,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionMethod {
    ChargeAutomatically,
    SendInvoice,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateInvoiceFromInvoiceAction {
    Revision,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateInvoiceIssuerType {
    Account,
    #[serde(rename = "self")]
    Self_,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceCustomerTaxExempt {
    Exempt,
    None,
    Reverse,
}

#[derive(Debug, Deserialize)]
pub struct InvoicesResourceInvoiceTaxId {
    /// The type of the tax ID, one of `ad_nrt`, `ar_cuit`, `eu_vat`, `bo_tin`, `br_cnpj`, `br_cpf`,
    /// `cn_tin`, `co_nit`, `cr_tin`, `do_rcn`, `ec_ruc`, `eu_oss_vat`, `pe_ruc`, `ro_tin`,
    /// `rs_pib`, `sv_nit`, `uy_ruc`, `ve_rif`, `vn_tin`, `gb_vat`, `nz_gst`, `au_abn`, `au_arn`,
    /// `in_gst`, `no_vat`, `za_vat`, `ch_vat`, `mx_rfc`, `sg_uen`, `ru_inn`, `ru_kpp`, `ca_bn`,
    /// `hk_br`, `es_cif`, `tw_vat`, `th_vat`, `jp_cn`, `jp_rn`, `jp_trn`, `li_uid`, `my_itn`,
    /// `us_ein`, `kr_brn`, `ca_qst`, `ca_gst_hst`, `ca_pst_bc`, `ca_pst_mb`, `ca_pst_sk`, `my_sst`,
    /// `sg_gst`, `ae_trn`, `cl_tin`, `sa_vat`, `id_npwp`, `my_frp`, `il_vat`, `ge_vat`, `ua_vat`,
    /// `is_vat`, `bg_uic`, `hu_tin`, `si_tin`, `ke_pin`, `tr_tin`, `eg_tin`, `ph_tin`, or
    /// `unknown`.
    #[serde(rename = "type")]
    pub type_: String,
    /// The value of the tax ID.
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvoicesFromInvoice {
    /// The relation between this invoice and the cloned invoice.
    pub action: String,
    /// The invoice that was cloned.
    pub invoice: super::IdOrObject<String, Box<Invoice>>,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
    Uncollectible,
    Void,
}

impl From<InvoiceStatus> for api::InvoiceStatus {
    fn from(status: InvoiceStatus) -> Self {
        match status {
            InvoiceStatus::Draft => api::InvoiceStatus::Draft,
            InvoiceStatus::Open => api::InvoiceStatus::Open,
            InvoiceStatus::Paid => api::InvoiceStatus::Paid,
            InvoiceStatus::Uncollectible => api::InvoiceStatus::Uncollectible,
            InvoiceStatus::Void => api::InvoiceStatus::Void,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListInvoices<'a> {
    customer: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "expand[]")]
    expand: Option<&'static str>,
}

impl<'a> ListInvoices<'a> {
    pub fn new(customer_id: &'a str, expand_discounts: bool) -> Self {
        let expand = expand_discounts.then_some("data.total_discount_amounts.discount");
        Self {
            customer: customer_id,
            expand,
        }
    }
}

impl super::StripeEndpoint for ListInvoices<'_> {
    type Result = super::ListResponse<Invoice>;

    fn method(&self) -> hyper::Method {
        hyper::Method::GET
    }

    fn path(&self) -> String {
        "invoices".to_string()
    }

    fn query(&self) -> Option<&Self> {
        Some(self)
    }

    fn body(&self) -> Option<&Self> {
        None
    }
}

/// The resource representing a Stripe "InvoiceLineItem".
#[derive(Debug, Deserialize)]
pub struct InvoiceLineItem {
    /// Unique identifier for the object.
    pub id: String,
    /// The amount, in cents (or local equivalent).
    pub amount: i64,
    /// The integer amount in cents (or local equivalent) representing the amount for this line
    /// item, excluding all tax and discounts.
    pub amount_excluding_tax: Option<i64>,
    /// Three-letter [ISO currency code](https://www.iso.org/iso-4217-currency-codes.html), in
    /// lowercase.
    ///
    /// Must be a [supported currency](https://stripe.com/docs/currencies).
    pub currency: super::currency::Currency,
    /// An arbitrary string attached to the object.
    ///
    /// Often useful for displaying to users.
    pub description: Option<String>,
    // /// The amount of discount calculated per discount for this line item.
    // pub discount_amounts: Option<Vec<DiscountsResourceDiscountAmount>>,
    /// If true, discounts will apply to this line item.
    ///
    /// Always false for prorations.
    pub discountable: bool,
    /// The discounts applied to the invoice line item.
    ///
    /// Line item discounts are applied before invoice discounts. Use `expand[]=discounts` to expand
    /// each discount.
    pub discounts: Option<Vec<super::IdOrObject<String, super::discount::Discount>>>,
    // /// The ID of the [invoice item](https://stripe.com/docs/api/invoiceitems) associated with this line item if any.
    // pub invoice_item: Option<super::IdOrObject<String, InvoiceItem>>,
    /// Has the value `true` if the object exists in live mode or the value `false` if the object
    /// exists in test mode.
    pub livemode: bool,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an
    /// object.
    ///
    /// This can be useful for storing additional information about the object in a structured
    /// format. Note that for line items with `type=subscription` this will reflect the metadata of
    /// the subscription that caused the line item to be created.
    pub metadata: super::Metadata,
    pub period: Option<Period>,
    /// The plan of the subscription, if the line item is a subscription or a proration.
    pub plan: Option<super::plan::Plan>,
    /// The price of the line item.
    pub price: Option<super::price::Price>,
    /// Whether this is a proration.
    pub proration: bool,
    // /// Additional details for proration line items.
    // pub proration_details: Option<InvoicesResourceLineItemsProrationDetails>,
    /// The quantity of the subscription, if the line item is a subscription or a proration.
    pub quantity: Option<u64>,
    // /// The subscription that the invoice item pertains to, if any.
    // pub subscription: Option<super::IdOrObject<String, Subscription>>,
    // /// The subscription item that generated this line item.
    // ///
    // /// Left empty if the line item is not an explicit result of a subscription.
    // pub subscription_item: Option<super::IdOrObject<String, SubscriptionItem>>,
    // /// The amount of tax calculated per tax rate for this line item.
    // pub tax_amounts: Option<Vec<TaxAmount>>,
    // /// The tax rates which apply to the line item.
    // pub tax_rates: Option<Vec<TaxRate>>,
    // /// A string identifying the type of the source of this line item, either an `invoiceitem` or a
    // /// `subscription`.
    // #[serde(rename = "type")]
    // pub type_: InvoiceLineItemType,
    /// The amount in cents (or local equivalent) representing the unit amount for this line item, excluding all tax and discounts.
    pub unit_amount_excluding_tax: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Period {
    pub end: Option<super::Timestamp>,
    pub start: Option<super::Timestamp>,
}

impl TryFrom<Invoice> for api::Invoice {
    type Error = Error;

    fn try_from(invoice: Invoice) -> Result<Self, Self::Error> {
        let discount_amounts = match &invoice.total_discount_amounts {
            Some(discounts) => discounts.iter().map(|da| (&da.discount.id, da)).collect(),
            None => HashMap::new(),
        };

        let convert_discounts = |discounts: Vec<
            super::IdOrObject<String, super::discount::Discount>,
        >| {
            discounts.iter()
                .map(|id_or_discount| match id_or_discount {
                    super::IdOrObject::Id(id) => id,
                    super::IdOrObject::Object(discount) => &discount.id,
                })
                .filter_map(|id| {
                    discount_amounts.get(id).or_else(|| {
                        tracing::warn!("Discount {id} referenced in response but not in `total_discount_amounts`");
                        None
                    })
                })
                .map(std::ops::Deref::deref)
                .map(api::Discount::try_from)
                .collect::<Result<_, _>>()
        };

        Ok(api::Invoice {
            number: invoice.number,
            created_at: invoice
                .created
                .and_then(|created| chrono::DateTime::from_timestamp(created.0, 0))
                .map(NanosUtc::from)
                .map(Into::into),
            discount: convert_discounts(invoice.discounts.unwrap_or_default())?,
            pdf_url: invoice.invoice_pdf,
            line_items: invoice
                .lines
                .map(|lines| lines.data)
                .unwrap_or_default()
                .into_iter()
                .map(|item| {
                    Ok(api::LineItem {
                        subtotal: item.amount,
                        unit_amount: item.price.and_then(|p| p.unit_amount),
                        description: item.description,
                        start: item
                            .period
                            .as_ref()
                            .and_then(|p| p.start.as_ref())
                            .and_then(|start| chrono::DateTime::from_timestamp(start.0, 0))
                            .map(NanosUtc::from)
                            .map(Into::into),
                        end: item
                            .period
                            .as_ref()
                            .and_then(|p| p.end.as_ref())
                            .and_then(|end| chrono::DateTime::from_timestamp(end.0, 0))
                            .map(NanosUtc::from)
                            .map(Into::into),
                        plan: item.plan.and_then(|plan| plan.nickname),
                        proration: item.proration,
                        quantity: item.quantity,
                        discounts: convert_discounts(item.discounts.unwrap_or_default())?,
                    })
                })
                .collect::<Result<_, Error>>()?,
            status: invoice
                .status
                .map(|status| api::InvoiceStatus::from(status) as i32),
            subtotal: invoice.subtotal,
            total: invoice.total,
        })
    }
}
