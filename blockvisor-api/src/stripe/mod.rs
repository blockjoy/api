pub mod api;
mod client;

use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;

use crate::auth::resource::OrgId;
use crate::models;
use crate::{auth::resource::UserId, config::stripe::Config};
use api::{customer, payment_method, price, setup_intent, subscription};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create stripe Client: {0}
    AttachPaymentMethod(client::Error),
    /// Failed to create stripe Client: {0}
    CreateClient(client::Error),
    /// Failed to create stripe customer: {0}
    CreateCustomer(client::Error),
    /// Failed to create stripe setup intent: {0}
    CreateSetupIntent(client::Error),
    /// Failed to create stripe subscription: {0}
    CreateSubscription(client::Error),
    /// Failed to create stripe subscription item: {0}
    CreateSubscriptionItem(client::Error),
    /// Failed to list stripe payment methods: {0}
    ListPaymentMethods(client::Error),
    /// Failed to list stripe susbcriptions: {0}
    ListSubscriptions(client::Error),
    /// No price found on stripe for sku `{0}`.
    NoPrice(String),
    /// Failed to search stripe prices: {0}
    SearchPrices(client::Error),
}

pub struct Stripe {
    pub config: Arc<Config>,
    pub client: client::Client,
}

#[tonic::async_trait]
pub trait Payment {
    async fn create_setup_intent(
        &self,
        org_id: OrgId,
        user_id: UserId,
    ) -> Result<setup_intent::SetupIntent, Error>;

    async fn create_customer(
        &self,
        org: &models::Org,
        payment_method_id: &api::PaymentMethodId,
    ) -> Result<customer::Customer, Error>;

    /// Attaches a payment method to a particular customer.
    async fn attach_payment_method(
        &self,
        payment_method_id: &api::PaymentMethodId,
        customer_id: &str,
    ) -> Result<payment_method::PaymentMethod, Error>;

    async fn list_payment_methods(
        &self,
        customer_id: &str,
    ) -> Result<Vec<payment_method::PaymentMethod>, Error>;

    async fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &price::PriceId,
    ) -> Result<subscription::Subscription, Error>;

    async fn create_item(
        &self,
        susbcription_id: &subscription::SubscriptionId,
        price_id: &price::PriceId,
    ) -> Result<subscription::SubscriptionItem, Error>;

    /// Each org only has one subscription.
    async fn get_subscription(
        &self,
        customer_id: &str,
    ) -> Result<Option<subscription::Subscription>, Error>;

    async fn get_price(&self, sku: &str) -> Result<price::Price, Error>;
}

impl Stripe {
    pub fn new(config: Arc<Config>) -> Result<Self, Error> {
        let client =
            client::Client::new(&config.secret, &config.base_url).map_err(Error::CreateClient)?;

        Ok(Self { config, client })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mock(config: Arc<Config>, server_url: url::Url) -> Result<Self, Error> {
        let client = client::Client::new_mock(server_url).map_err(Error::CreateClient)?;
        Ok(Self { config, client })
    }
}

#[tonic::async_trait]
impl Payment for Stripe {
    async fn create_setup_intent(
        &self,
        org_id: OrgId,
        user_id: UserId,
    ) -> Result<setup_intent::SetupIntent, Error> {
        let req = setup_intent::CreateSetupIntent::new(org_id, user_id);
        self.client
            .request(&req)
            .await
            .map_err(Error::CreateSetupIntent)
    }

    async fn create_customer(
        &self,
        org: &models::Org,
        payment_method_id: &api::PaymentMethodId,
    ) -> Result<customer::Customer, Error> {
        let customer = customer::CreateCustomer::new(org, payment_method_id);
        self.client
            .request(&customer)
            .await
            .map_err(Error::CreateCustomer)
    }

    async fn attach_payment_method(
        &self,
        payment_method_id: &api::PaymentMethodId,
        customer_id: &str,
    ) -> Result<payment_method::PaymentMethod, Error> {
        let attach = payment_method::AttachPaymentMethod::new(payment_method_id, customer_id);
        self.client
            .request(&attach)
            .await
            .map_err(Error::AttachPaymentMethod)
    }

    async fn list_payment_methods(
        &self,
        customer_id: &str,
    ) -> Result<Vec<payment_method::PaymentMethod>, Error> {
        let req = payment_method::ListPaymentMethodsRequest::new(customer_id);
        let resp = self
            .client
            .request(&req)
            .await
            .map_err(Error::ListPaymentMethods)?;
        Ok(resp.data)
    }

    async fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &price::PriceId,
    ) -> Result<subscription::Subscription, Error> {
        let req = subscription::CreateSubscription::new(customer_id, price_id);
        self.client
            .request(&req)
            .await
            .map_err(Error::CreateSubscription)
    }

    async fn create_item(
        &self,
        susbcription_id: &subscription::SubscriptionId,
        price_id: &price::PriceId,
    ) -> Result<subscription::SubscriptionItem, Error> {
        let req = subscription::CreateSubscriptionItem::new(susbcription_id, price_id);
        self.client
            .request(&req)
            .await
            .map_err(Error::CreateSubscriptionItem)
    }

    async fn get_subscription(
        &self,
        customer_id: &str,
    ) -> Result<Option<subscription::Subscription>, Error> {
        let req = subscription::ListSubscriptions::new(customer_id);
        let mut subscriptions = self
            .client
            .request(&req)
            .await
            .map_err(Error::ListSubscriptions)?
            .data;
        if let Some(subscription) = subscriptions.pop() {
            if !subscriptions.is_empty() {
                tracing::warn!("More than one subscription returned for customer `{customer_id}`.");
            }
            Ok(Some(subscription))
        } else {
            Ok(None)
        }
    }

    async fn get_price(&self, sku: &str) -> Result<price::Price, Error> {
        let req = price::SearchPrice::new(sku);
        let mut prices = self
            .client
            .request(&req)
            .await
            .map_err(Error::SearchPrices)?
            .data;
        if let Some(price) = prices.pop() {
            if !prices.is_empty() {
                tracing::warn!("More than one price returned for sku `{sku}`.");
            }
            Ok(price)
        } else {
            Err(Error::NoPrice(sku.to_string()))
        }
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use mockito::ServerGuard;

    use super::*;

    pub struct MockStripe {
        pub server: ServerGuard,
        pub stripe: Stripe,
    }

    #[tonic::async_trait]
    impl Payment for MockStripe {
        async fn create_setup_intent(
            &self,
            org_id: OrgId,
            user_id: UserId,
        ) -> Result<setup_intent::SetupIntent, Error> {
            self.stripe.create_setup_intent(org_id, user_id).await
        }

        async fn create_customer(
            &self,
            org: &models::Org,
            payment_method_id: &api::PaymentMethodId,
        ) -> Result<customer::Customer, Error> {
            self.stripe.create_customer(org, payment_method_id).await
        }

        async fn attach_payment_method(
            &self,
            payment_method_id: &api::PaymentMethodId,
            customer_id: &str,
        ) -> Result<payment_method::PaymentMethod, Error> {
            self.stripe
                .attach_payment_method(payment_method_id, customer_id)
                .await
        }

        async fn list_payment_methods(
            &self,
            customer_id: &str,
        ) -> Result<Vec<payment_method::PaymentMethod>, Error> {
            self.stripe.list_payment_methods(customer_id).await
        }

        async fn create_subscription(
            &self,
            customer_id: &str,
            price_id: &price::PriceId,
        ) -> Result<subscription::Subscription, Error> {
            self.stripe.create_subscription(customer_id, price_id).await
        }

        async fn create_item(
            &self,
            susbcription_id: &subscription::SubscriptionId,
            price_id: &price::PriceId,
        ) -> Result<subscription::SubscriptionItem, Error> {
            self.stripe.create_item(susbcription_id, price_id).await
        }

        /// Each org only has one subscription.
        async fn get_subscription(
            &self,
            customer_id: &str,
        ) -> Result<Option<subscription::Subscription>, Error> {
            self.stripe.get_subscription(customer_id).await
        }

        async fn get_price(&self, sku: &str) -> Result<price::Price, Error> {
            self.stripe.get_price(sku).await
        }
    }

    impl MockStripe {
        pub async fn new() -> Self {
            let server = mock_server().await;
            let server_url = server.url().parse().unwrap();
            let config = Arc::new(mock_config(&server));
            let stripe = Stripe::new_mock(config, server_url).unwrap();

            Self { server, stripe }
        }
    }

    async fn mock_server() -> ServerGuard {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("POST", "https://api.stripe.com/v1/setup_intents")
            .with_status(200)
            .with_body(mock_setup_intent())
            .create_async()
            .await;

        server
            .mock("GET", "https://api.stripe.com/v1/prices/search")
            .with_status(200)
            .with_body(mock_prices())
            .create_async()
            .await;

        server
    }

    fn mock_config(server: &ServerGuard) -> Config {
        Config {
            secret: "stripe_fake_secret".to_owned().into(),
            base_url: server.url(),
        }
    }

    const fn mock_setup_intent() -> &'static str {
        r#"{
          "id": "seti_1PIt1LB5ce1jJsfThXFVl6TA",
          "object": "setup_intent",
          "application": null,
          "automatic_payment_methods": null,
          "cancellation_reason": null,
          "client_secret": "seti_1PIt1LB5ce1jJsfThXFVl6TA_secret_Q9BOXjYJe26wDp1MJs4Yx6va95vOSJv",
          "created": 1716299187,
          "customer": null,
          "description": null,
          "flow_directions": null,
          "last_setup_error": null,
          "latest_attempt": null,
          "livemode": false,
          "mandate": null,
          "metadata": {},
          "next_action": null,
          "on_behalf_of": null,
          "payment_method": null,
          "payment_method_configuration_details": null,
          "payment_method_options": {
            "card": {
              "mandate_options": null,
              "network": null,
              "request_three_d_secure": "automatic"
            }
          },
          "payment_method_types": [
            "card"
          ],
          "single_use_mandate": null,
          "status": "requires_payment_method",
          "usage": "off_session"
        }"#
    }

    const fn mock_prices() -> &'static str {
        r#"{
          "object": "search_result",
          "url": "/v1/prices/search",
          "has_more": false,
          "data": [
            {
              "id": "price_1MoBy5LkdIwHu7ixZhnattbh",
              "object": "price",
              "active": true,
              "billing_scheme": "per_unit",
              "created": 1679431181,
              "currency": "usd",
              "custom_unit_amount": null,
              "livemode": false,
              "lookup_key": null,
              "metadata": {
                "order_id": "6735"
              },
              "nickname": null,
              "product": "prod_NZKdYqrwEYx6iK",
              "recurring": {
                "aggregate_usage": null,
                "interval": "month",
                "interval_count": 1,
                "trial_period_days": null,
                "usage_type": "licensed"
              },
              "tax_behavior": "unspecified",
              "tiers_mode": null,
              "transform_quantity": null,
              "type": "recurring",
              "unit_amount": 1000,
              "unit_amount_decimal": "1000"
            }
          ],
        }"#
    }
}
