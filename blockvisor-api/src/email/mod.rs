pub mod template;
pub use template::{Kind, Language, Templates};

use std::collections::HashMap;
use std::sync::Arc;

use displaydoc::Display;
use sendgrid::v3;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::auth::claims::Claims;
use crate::auth::rbac::EmailRole;
use crate::auth::resource::Resource;
use crate::auth::token::Cipher;
use crate::config::Config;
use crate::config::token::ExpireChrono;
use crate::model::{Invitation, User};

const FROM_EMAIL: &str = "no-reply@blockjoy.com";
const FROM_NAME: &str = "BlockJoy";

#[tonic::async_trait]
pub trait Sender {
    async fn send_mail(&self, mail: v3::Message) -> Result<(), Error>;
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
    pub fn new(config: &Config, cipher: Arc<Cipher>) -> Result<Option<Self>, Error> {
        let Some((sendgrid_api_key, template_dir)) = config
            .email
            .sendgrid_api_key
            .as_deref()
            .zip(config.email.template_dir.as_deref())
        else {
            return Ok(None);
        };

        let sender = Box::new(v3::Sender::new(sendgrid_api_key.clone(), None));
        let templates = Templates::new(template_dir)?;
        let base_url = config.email.ui_base_url.clone();
        let expires = config.token.expire;

        Ok(Some(Email {
            sender,
            templates,
            cipher,
            base_url,
            expires,
        }))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub fn new_mocked(config: &Config, cipher: Arc<Cipher>) -> Result<Self, Error> {
        let sender = Box::new(tests::MockEmail {});
        let templates = Templates::new(config.email.template_dir.as_deref().unwrap())?;
        let base_url = config.email.ui_base_url.clone();
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
            claims.insert_data("invitation_id", id.to_string());
        }

        let token = self.cipher.jwt.encode(&claims).map_err(Error::EncodeJwt)?;

        let base = &self.base_url;
        let context = hashmap! {
            "link" => format!("{base}/verified?token={}", *token),
        };

        self.send(Kind::RegistrationConfirmation, user, Some(context))
            .await
    }

    pub async fn invitation_for_registered<S>(
        &self,
        invitation: &Invitation,
        inviter: String,
        invitee: &User,
        expiration: S,
    ) -> Result<(), Error>
    where
        S: ToString + Send,
    {
        let base = &self.base_url;
        let context = hashmap! {
            "inviter" => inviter,
            "link" => format!("{base}/invite-registered?invitation_id={}", invitation.id),
            "decline_link" => format!("{base}/decline-registered?invitation_id={}", invitation.id),
            "expiration" => expiration.to_string()
        };

        self.send(Kind::InviteRegistered, invitee, Some(context))
            .await
    }

    pub async fn invitation<S>(
        &self,
        invitation: &Invitation,
        inviter: String,
        invitee: Recipient<'_>,
        expiration: S,
    ) -> Result<(), Error>
    where
        S: ToString + Send,
    {
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
            "inviter" => inviter,
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
        let mut claims = Claims::from_now(expires, user.id, EmailRole::ResetPassword);
        claims.data = Some(hashmap! {
            "email".to_string() => user.email.clone(),
        });
        let token = self.cipher.jwt.encode(&claims).map_err(Error::EncodeJwt)?;

        let base = &self.base_url;
        let context = hashmap! {
            "link" => format!("{base}/password-reset?token={}", *token)
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
        R: Into<Recipient<'r>> + Send,
    {
        let recipient = recipient.into();
        let name = recipient.name();
        let lang = recipient.preferred_language.unwrap_or(Language::En);
        let template = self.templates.render(kind, lang, context)?;

        let to = v3::Email::new(recipient.email).set_name(name);
        let from = v3::Email::new(FROM_EMAIL).set_name(FROM_NAME);
        let text = v3::Content::new()
            .set_content_type("text/plain")
            .set_value(template.text);
        let html = v3::Content::new()
            .set_content_type("text/html")
            .set_value(template.html);
        let mail = v3::Message::new(from)
            .add_personalization(v3::Personalization::new(to))
            .set_subject(kind.subject())
            .add_content(text)
            .add_content(html)
            .set_tracking_settings(Self::tracking_settings());

        self.sender.send_mail(mail).await
    }

    const fn tracking_settings() -> v3::TrackingSettings {
        v3::TrackingSettings {
            click_tracking: Some(v3::ClickTrackingSetting {
                enable: Some(false),
                enable_text: Some(false),
            }),
            open_tracking: None,
            subscription_tracking: None,
        }
    }
}

#[tonic::async_trait]
impl Sender for v3::Sender {
    async fn send_mail(&self, mail: v3::Message) -> Result<(), Error> {
        self.send(&mail).await.map(|_| ()).map_err(Error::SendMail)
    }
}

pub struct Recipient<'r> {
    pub first_name: &'r str,
    pub last_name: &'r str,
    pub email: &'r str,
    pub preferred_language: Option<Language>,
}

impl Recipient<'_> {
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
            preferred_language: Some(Language::En),
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
        async fn send_mail(&self, _mail: v3::Message) -> Result<(), Error> {
            debug!("Mocked email");
            Ok(())
        }
    }
}
