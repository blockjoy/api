use std::any::type_name;

/// The resource representing a Stripe "PaymentMethod".
///
/// For more details see <https://stripe.com/docs/api/payment_methods/object>
#[derive(Debug, serde::Deserialize)]
pub struct PaymentMethod {
    /// Unique identifier for the object.
    pub id: super::PaymentMethodId,
    // pub acss_debit: Option<PaymentMethodAcssDebit>,
    // pub affirm: Option<PaymentMethodAffirm>,
    // pub afterpay_clearpay: Option<PaymentMethodAfterpayClearpay>,
    // pub alipay: Option<PaymentFlowsPrivatePaymentMethodsAlipay>,
    // pub au_becs_debit: Option<PaymentMethodAuBecsDebit>,
    // pub bacs_debit: Option<PaymentMethodBacsDebit>,
    // pub bancontact: Option<PaymentMethodBancontact>,
    // pub billing_details: BillingDetails,
    // pub blik: Option<PaymentMethodBlik>,
    // pub boleto: Option<PaymentMethodBoleto>,
    // pub card: Option<CardDetails>,
    // pub card_present: Option<CardPresent>,
    // pub cashapp: Option<PaymentMethodCashapp>,
    // /// Time at which the object was created.
    // ///
    // /// Measured in seconds since the Unix epoch.
    // pub created: Timestamp,
    // /// The ID of the Customer to which this PaymentMethod is saved.
    // ///
    // /// This will not be set when the PaymentMethod has not been saved to a Customer.
    // pub customer: Option<Expandable<Customer>>,
    // pub customer_balance: Option<PaymentMethodCustomerBalance>,
    // pub eps: Option<PaymentMethodEps>,
    // pub fpx: Option<PaymentMethodFpx>,
    // pub giropay: Option<PaymentMethodGiropay>,
    // pub grabpay: Option<PaymentMethodGrabpay>,
    // pub ideal: Option<PaymentMethodIdeal>,
    // pub interac_present: Option<PaymentMethodInteracPresent>,
    // pub klarna: Option<PaymentMethodKlarna>,
    // pub konbini: Option<PaymentMethodKonbini>,
    // pub link: Option<PaymentMethodLink>,
    // /// Has the value `true` if the object exists in live mode or the value `false` if the object exists in test mode.
    // pub livemode: bool,
    // /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an object.
    // ///
    // /// This can be useful for storing additional information about the object in a structured format.
    // pub metadata: Option<Metadata>,
    // pub oxxo: Option<PaymentMethodOxxo>,
    // pub p24: Option<PaymentMethodP24>,
    // pub paynow: Option<PaymentMethodPaynow>,
    // pub paypal: Option<PaymentMethodPaypal>,
    // pub pix: Option<PaymentMethodPix>,
    // pub promptpay: Option<PaymentMethodPromptpay>,
    // pub radar_options: Option<RadarRadarOptions>,
    // pub revolut_pay: Option<PaymentMethodRevolutPay>,
    // pub sepa_debit: Option<PaymentMethodSepaDebit>,
    // pub sofort: Option<PaymentMethodSofort>,
    // pub swish: Option<PaymentMethodSwish>,
    // /// The type of the PaymentMethod.
    // ///
    // /// An additional hash is included on the PaymentMethod with a name matching this value.
    // /// It contains additional information specific to the PaymentMethod type.
    // #[serde(rename = "type")]
    // pub type_: PaymentMethodType,
    // pub us_bank_account: Option<PaymentMethodUsBankAccount>,
    // pub wechat_pay: Option<PaymentMethodWechatPay>,
    // pub zip: Option<PaymentMethodZip>,
}

#[derive(Debug, serde::Serialize)]
pub struct AttachPaymentMethod<'a> {
    #[serde(skip_serializing)]
    payment_method_id: &'a super::PaymentMethodId,
    customer: &'a str,
}

impl<'a> AttachPaymentMethod<'a> {
    pub const fn new(payment_method_id: &'a super::PaymentMethodId, customer: &'a str) -> Self {
        Self {
            payment_method_id,
            customer,
        }
    }
}

impl super::StripeEndpoint for AttachPaymentMethod<'_> {
    type Result = PaymentMethod;

    fn method(&self) -> hyper::Method {
        hyper::Method::POST
    }

    fn path(&self) -> String {
        format!("payment_methods/{}/attach", self.payment_method_id)
    }

    fn body(&self) -> Option<String> {
        serde_json::to_string(self)
            .map_err(|err| tracing::warn!("Failed to serialize {}: {}", type_name::<Self>(), err))
            .ok()
    }
}
