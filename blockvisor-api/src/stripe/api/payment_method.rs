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
    pub billing_details: BillingDetails,
    // pub blik: Option<PaymentMethodBlik>,
    // pub boleto: Option<PaymentMethodBoleto>,
    pub card: Option<super::card::CardDetails>,
    // pub card_present: Option<CardPresent>,
    // pub cashapp: Option<PaymentMethodCashapp>,
    /// Time at which the object was created.
    ///
    /// Measured in seconds since the Unix epoch.
    pub created: super::Timestamp,
    /// The ID of the Customer to which this PaymentMethod is saved.
    ///
    /// This will not be set when the PaymentMethod has not been saved to a Customer.
    pub customer: Option<super::IdOrObject<String, super::customer::Customer>>,
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
    /// Has the value `true` if the object exists in live mode or the value `false` if the object exists in test mode.
    pub livemode: bool,
    /// Set of [key-value pairs](https://stripe.com/docs/api/metadata) that you can attach to an object.
    ///
    /// This can be useful for storing additional information about the object in a structured format.
    pub metadata: Option<super::Metadata>,
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

/// The resource representing a Stripe "billing_details".
#[derive(Debug, serde::Deserialize)]
pub struct BillingDetails {
    /// Billing address.
    pub address: Option<super::Address>,
    /// Email address.
    pub email: Option<String>,
    /// Full name.
    pub name: Option<String>,
    /// Billing phone number (including extension).
    pub phone: Option<String>,
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

    fn body(&self) -> Option<&Self> {
        Some(self)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ListPaymentMethodsRequest<'a> {
    customer: &'a str,
}

impl<'a> ListPaymentMethodsRequest<'a> {
    pub const fn new(customer: &'a str) -> Self {
        Self { customer }
    }
}

impl super::StripeEndpoint for ListPaymentMethodsRequest<'_> {
    type Result = super::ListResponse<PaymentMethod>;

    fn method(&self) -> hyper::Method {
        hyper::Method::GET
    }

    fn path(&self) -> String {
        "payment_methods".to_string()
    }

    fn query(&self) -> Option<&Self> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::stripe::api::ListResponse;

    use super::*;

    #[test]
    fn can_parse_sample_response() {
        let sample = r#"{
          "object": "list",
          "data": [
            {
              "id": "pm_1PGye7B5ce1jJsfTAJl2xjNs",
              "object": "payment_method",
              "allow_redisplay": "always",
              "billing_details": {
                "address": {
                  "city": null,
                  "country": null,
                  "line1": null,
                  "line2": null,
                  "postal_code": null,
                  "state": null
                },
                "email": null,
                "name": null,
                "phone": null
              },
              "card": {
                "brand": "mastercard",
                "checks": {
                  "address_line1_check": null,
                  "address_postal_code_check": null,
                  "cvc_check": "pass"
                },
                "country": "US",
                "display_brand": "mastercard",
                "exp_month": 3,
                "exp_year": 2030,
                "fingerprint": "VAFyrymPOHSx4RE3",
                "funding": "credit",
                "generated_from": null,
                "last4": "4444",
                "networks": {
                  "available": [
                    "mastercard"
                  ],
                  "preferred": null
                },
                "three_d_secure_usage": {
                  "supported": true
                },
                "wallet": null
              },
              "created": 1715844155,
              "customer": "cus_Q7D3Rr4wadZ1WX",
              "livemode": false,
              "metadata": {},
              "radar_options": {},
              "type": "card"
            },
            {
              "id": "pm_1PGydqB5ce1jJsfTMwn997f5",
              "object": "payment_method",
              "allow_redisplay": "always",
              "billing_details": {
                "address": {
                  "city": null,
                  "country": "US",
                  "line1": null,
                  "line2": null,
                  "postal_code": null,
                  "state": null
                },
                "email": null,
                "name": "Dragan Rakita",
                "phone": null
              },
              "card": {
                "brand": "visa",
                "checks": {
                  "address_line1_check": null,
                  "address_postal_code_check": null,
                  "cvc_check": "pass"
                },
                "country": "US",
                "display_brand": "visa",
                "exp_month": 3,
                "exp_year": 2030,
                "fingerprint": "6miHob5DrPy3VE1b",
                "funding": "credit",
                "generated_from": null,
                "last4": "4242",
                "networks": {
                  "available": [
                    "visa"
                  ],
                  "preferred": null
                },
                "three_d_secure_usage": {
                  "supported": true
                },
                "wallet": null
              },
              "created": 1715844138,
              "customer": "cus_Q7D3Rr4wadZ1WX",
              "livemode": false,
              "metadata": {},
              "radar_options": {},
              "type": "card"
            }
          ],
          "has_more": false,
          "url": "/v1/payment_methods"
        }"#;
        let _: ListResponse<PaymentMethod> = serde_json::from_str(sample).unwrap();
    }
}
