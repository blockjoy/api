pub mod api;
mod client;

use std::sync::Arc;

use displaydoc::Display;
use thiserror::Error;

use crate::auth::resource::OrgId;
use crate::models;
use crate::{auth::resource::UserId, config::stripe::Config};
use api::{customer, payment_method, setup_intent};

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
    /// Failed to list stripe payment methods: {0}
    ListPaymentMethods(client::Error),
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
}
