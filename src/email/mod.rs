pub mod template;
pub use template::{Kind, Language, Template, Templates};

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use displaydoc::Display;
use sendgrid::{Destination, Mail, SGClient};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::auth::claims::Claims;
use crate::auth::rbac::EmailRole;
use crate::auth::resource::Resource;
use crate::auth::token::Cipher;
use crate::config::token::ExpireChrono;
use crate::config::Config;
use crate::models::{Invitation, User};

const FROM_EMAIL: &str = "no-reply@blockjoy.com";
const FROM_NAME: &str = "BlockJoy";

#[tonic::async_trait]
pub trait Sender {
    async fn send_mail(&self, mail: Mail<'_>) -> Result<(), Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to encode JWT: {0}
    EncodeJwt(crate::auth::token::jwt::Error),
    /// Failed to send email: {0}
    SendMail(sendgrid::SendgridError),
    /// Template error: {0}
    Template(#[from] template::Error),
}

pub struct Email {
    sender: Box<dyn Sender + Send + Sync + 'static>,
    templates: Templates,
    cipher: Arc<Cipher>,
    base_url: Url,
    expires: ExpireChrono,
}

impl Email {
    pub fn new(config: &Config, cipher: Arc<Cipher>) -> Result<Self, Error> {
        let sender = Box::new(SGClient::new(&*config.mail.sendgrid_api_key));
        let templates = Templates::new()?;
        let base_url = config.mail.ui_base_url.clone();
        let expires = config.token.expire;

        Ok(Email {
            sender,
            templates,
            cipher,
            base_url,
            expires,
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mocked(config: &Config, cipher: Arc<Cipher>) -> Result<Self, Error> {
        let sender = Box::new(tests::MockEmail {});
        let templates = Templates::new()?;
        let base_url = config.mail.ui_base_url.clone();
        let expires = config.token.expire;

        Ok(Email {
            sender,
            templates,
            cipher,
            base_url,
            expires,
        })
    }

    /// Sends a confirmation if the user has updated their password.
    pub async fn update_password(&self, user: &User) -> Result<(), Error> {
        self.send(Kind::UpdatePassword, user, None).await
    }

    pub async fn registration_confirmation(
        &self,
        user: &User,
        invitation_id: Option<Uuid>,
    ) -> Result<(), Error> {
        let expires = self.expires.registration_confirmation;
        let mut claims = Claims::from_now(expires, user.id, EmailRole::RegistrationConfirmation);
        if let Some(id) = invitation_id {
            claims.insert_data("invitation_id", id)
        }

        let token = self.cipher.jwt.encode(&claims).map_err(Error::EncodeJwt)?;

        let base = &self.base_url;
        let context = hashmap! {
            "link" => format!("{base}/verified?token={}", *token),
        };

        self.send(Kind::RegistrationConfirmation, user, Some(context))
            .await
    }

    pub async fn invitation_for_registered(
        &self,
        invitation: &Invitation,
        inviter: &User,
        invitee: &User,
        expiration: impl std::fmt::Display,
    ) -> Result<(), Error> {
        let base = &self.base_url;
        let context = hashmap! {
            "inviter" => format!("{} ({})", inviter.name(), inviter.email),
            "link" => format!("{base}/invite-registered?invitation_id={}", invitation.id),
            "decline_link" => format!("{base}/decline-registered?invitation_id={}", invitation.id),
            "expiration" => expiration.to_string()
        };

        self.send(Kind::InviteRegistered, invitee, Some(context))
            .await
    }

    pub async fn invitation(
        &self,
        invitation: &Invitation,
        inviter: &User,
        invitee: Recipient<'_>,
        expiration: impl std::fmt::Display,
    ) -> Result<(), Error> {
        let resource = Resource::Org(invitation.org_id);
        let expires = self.expires.invitation;
        let data = hashmap! {
            "email".to_string() => invitee.email.to_owned(),
            "invitation_id".to_string() => invitation.id.to_string(),
        };
        let claims = Claims::from_now(expires, resource, EmailRole::Invitation).with_data(data);
        let token = self.cipher.jwt.encode(&claims).map_err(Error::EncodeJwt)?;

        let base = &self.base_url;
        let context = hashmap! {
            "inviter" => format!("{} ({})", inviter.name(), inviter.email),
            "accept_link" => format!("{base}/accept-invite?token={}", *token),
            "decline_link" => format!("{base}/decline-invite?token={}", *token),
            "expiration" => expiration.to_string(),
        };

        self.send(Kind::InviteUser, invitee, Some(context)).await
    }

    /// Sends a password reset email to the specified user containing a JWT that
    /// they can use to authenticate themselves to reset their password.
    pub async fn reset_password(&self, user: &User) -> Result<(), Error> {
        let expires = self.expires.password_reset;
        let claims = Claims::from_now(expires, user.id, EmailRole::ResetPassword);
        let token = self.cipher.jwt.encode(&claims).map_err(Error::EncodeJwt)?;

        let base = &self.base_url;
        let context = hashmap! {
            "link" => format!("{base}/password_reset?token={}", *token)
        };

        self.send(Kind::ResetPassword, user, Some(context)).await
    }

    async fn send<'r, R>(
        &self,
        kind: Kind,
        recipient: R,
        context: Option<HashMap<&'static str, String>>,
    ) -> Result<(), Error>
    where
        R: Into<Recipient<'r>>,
    {
        let recipient = recipient.into();
        let name = recipient.name();
        let lang = recipient.preferred_language.unwrap_or(Language::En);
        let template = self.templates.render(kind, lang, context)?;

        let mail = Mail {
            to: vec![Destination {
                address: recipient.email,
                name: &name,
            }],
            from: FROM_EMAIL,
            subject: kind.subject(),
            html: &template.html,
            text: &template.text,
            from_name: FROM_NAME,
            date: &Utc::now().to_rfc2822(),
            ..Default::default()
        };

        self.sender.send_mail(mail).await
    }
}

#[tonic::async_trait]
impl Sender for SGClient {
    async fn send_mail(&self, mail: Mail<'_>) -> Result<(), Error> {
        self.send(mail).await.map(|_| ()).map_err(Error::SendMail)
    }
}

pub struct Recipient<'r> {
    pub first_name: &'r str,
    pub last_name: &'r str,
    pub email: &'r str,
    pub preferred_language: Option<Language>,
}

impl<'r> Recipient<'r> {
    pub fn name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}

impl<'r> From<&'r User> for Recipient<'r> {
    fn from(user: &'r User) -> Self {
        Recipient {
            first_name: &user.first_name,
            last_name: &user.last_name,
            email: &user.email,
            preferred_language: Some(user.preferred_language()),
        }
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use tracing::debug;

    use super::*;

    pub struct MockEmail;

    #[tonic::async_trait]
    impl Sender for MockEmail {
        async fn send_mail(&self, mail: Mail<'_>) -> Result<(), Error> {
            debug!("Mocked email: {:?}", mail);
            Ok(())
        }
    }
}
