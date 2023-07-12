use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use tracing::warn;
use url::Url;

use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::auth::token::Cipher;
use crate::config::{token, Config};
use crate::models::{Invitation, User};

pub struct MailClient {
    client: sendgrid::SGClient,
    token_config: Arc<token::Config>,
    base_url: Url,
    cipher: Arc<Cipher>,
}

impl MailClient {
    pub fn new(config: &Config, cipher: Arc<Cipher>) -> Self {
        Self {
            client: sendgrid::SGClient::new(&*config.mail.sendgrid_api_key),
            token_config: config.token.clone(),
            base_url: config.mail.ui_base_url.clone(),
            cipher,
        }
    }

    /// Sends a notification if the user has updated his password
    pub async fn update_password(&self, user: &User) -> crate::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/update_password.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        self.send_mail(
            &templates,
            Recipient::redact_user(user),
            "[BlockJoy] Password Updated".to_string(),
            None,
        )
        .await
    }

    pub async fn registration_confirmation(&self, user: &User) -> crate::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/register.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        let expires = self
            .token_config
            .expire
            .registration_confirmation
            .try_into()?;
        let endpoints = [Endpoint::AuthConfirm];
        let claims = Claims::user_from_now(expires, user.id, endpoints);

        let token = self.cipher.jwt.encode(&claims)?;
        let link = format!("{}/verified?token={}", self.base_url, *token);
        let mut context = HashMap::new();
        context.insert("link".to_owned(), link);

        self.send_mail(
            &templates,
            Recipient::redact_user(user),
            "[BlockJoy] Verify Your Account".to_string(),
            Some(context),
        )
        .await
    }

    pub async fn invitation_for_registered(
        &self,
        inviter: &User,
        invitee: &User,
        expiration: impl std::fmt::Display,
    ) -> crate::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/invite_registered_user.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        let link = format!("{}/invite-registered?uid={}", self.base_url, *invitee.id);
        let inviter = format!(
            "{} {} ({})",
            inviter.first_name, inviter.last_name, inviter.email
        );
        let mut context = HashMap::new();
        context.insert("inviter".to_owned(), inviter);
        context.insert("link".to_owned(), link);
        context.insert("expiration".to_owned(), expiration.to_string());

        self.send_mail(
            &templates,
            Recipient::redact_user(invitee),
            "[BlockJoy] Organization Invite".to_string(),
            Some(context),
        )
        .await
    }

    pub async fn invitation(
        &self,
        invitation: &Invitation,
        inviter: &User,
        invitee: Recipient<'_>,
        expiration: impl std::fmt::Display,
    ) -> crate::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/invite_user.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        let expires = self.token_config.expire.invitation.try_into()?;
        let endpoints = [
            Endpoint::UserCreate,
            Endpoint::InvitationAccept,
            Endpoint::InvitationDecline,
        ];
        let mut data = HashMap::new();
        data.insert("email".to_string(), invitation.invitee_email.clone());
        data.insert("invitation_id".to_string(), invitation.id.to_string());
        data.insert("org_id".to_string(), invitation.org_id.to_string());

        // A little bit lame but not that big of a deal: the id of the invitation as the id of the
        // user because there is no user here yet.
        let resource = Resource::Org(invitation.org_id);
        let claims = Claims::from_now(expires, resource, endpoints);
        let token = self.cipher.jwt.encode(&claims)?;

        let accept_link = format!("{}/accept-invite?token={}", self.base_url, *token);
        let decline_link = format!("{}/decline-invite?token={}", self.base_url, *token);
        let inviter = format!(
            "{} {} ({})",
            inviter.first_name, inviter.last_name, inviter.email
        );
        let context = HashMap::from([
            ("inviter".to_owned(), inviter),
            ("accept_link".to_owned(), accept_link),
            ("decline_link".to_owned(), decline_link),
            ("expiration".to_owned(), expiration.to_string()),
        ]);

        self.send_mail(
            &templates,
            invitee,
            "[BlockJoy] Organization Invite".to_string(),
            Some(context),
        )
        .await
    }

    /// Sends a password reset email to the specified user, containing a JWT that they can use to
    /// authenticate themselves to reset their password.
    pub async fn reset_password(&self, user: &User) -> crate::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/reset_password.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        let expires = self.token_config.expire.password_reset.try_into()?;
        let endpoints = [Endpoint::AuthUpdatePassword];
        let claims = Claims::user_from_now(expires, user.id, endpoints);
        let token = self.cipher.jwt.encode(&claims)?;

        let link = format!("{}/password_reset?token={}", self.base_url, *token);
        let context = HashMap::from([("link".to_string(), link)]);

        self.send_mail(
            &templates,
            Recipient::redact_user(user),
            "[BlockJoy] Reset Password".to_string(),
            Some(context),
        )
        .await
    }

    async fn send_mail(
        &self,
        templates: &Templates,
        to: Recipient<'_>,
        subject: String,
        // Can't use 'static str for the keys or the values here, see:
        // https://stackoverflow.com/questions/68591843
        context: Option<HashMap<String, String>>,
    ) -> crate::Result<()> {
        let context = context.unwrap_or_default();
        let template = templates.by_lang(to.preferred_language());
        let (html, text) = template.render(context)?;

        let to = sendgrid::Destination {
            address: to.email,
            name: &format!("{} {}", to.first_name, to.last_name),
        };
        let mail = sendgrid::Mail {
            to: vec![to],
            from: "no-reply@blockjoy.com",
            subject: subject.as_str(),
            html: &html,
            text: &text,
            from_name: "BlockJoy",
            date: &chrono::Utc::now().to_rfc2822(),
            ..Default::default()
        };

        // TODO: better error handling
        if let Err(err) = self.client.send(mail).await {
            warn!("Failed to send email: {err}");
        }

        Ok(())
    }
}

pub struct Recipient<'a> {
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub email: &'a str,
    pub preferred_language: Option<&'a str>,
}

impl<'a> Recipient<'a> {
    fn preferred_language(&self) -> &str {
        self.preferred_language.unwrap_or("en")
    }

    fn redact_user(value: &'a User) -> Self {
        Self {
            first_name: &value.first_name,
            last_name: &value.last_name,
            email: &value.email,
            preferred_language: Some(value.preferred_language()),
        }
    }
}

/// A collection of templates that we have for a specific email. There are multiple because we
/// support multiple languages.
#[derive(serde::Deserialize)]
struct Templates {
    /// The default template, english, seperated out from the `templates` map below, so it missing
    /// will be a deserialization error caught by our tests.
    en: Template,
    /// Contains a mapping from language code (i.e. `de`, etc) to a template struct.
    #[serde(flatten)]
    templates: HashMap<String, Template>,
}

impl Templates {
    /// Retrieve the template by the language code. This does not return an Option because we
    /// fall back to english if the template is missing, and we expect to always be able to provide
    /// an english template.
    fn by_lang(&self, lang: &str) -> &Template {
        self.templates.get(lang).unwrap_or(&self.en)
    }
}

#[derive(serde::Deserialize)]
struct Template {
    html: String,
    text: String,
}

impl Template {
    /// Renders the contained templates to a tuple of strings. These are the rendered HTML email
    /// and the rendered plaintext email. The context argument is the list of parameters needed to
    /// render the email, imagine stuff like "first_name" => "Ebenezer".
    fn render(&self, context: HashMap<String, String>) -> crate::Result<(String, String)> {
        let renderer = Self::renderer();
        let html = renderer
            .render_template(&self.html, &context)
            .map_err(|e| anyhow!("Template failed to render: {e}"))?;
        let text = renderer
            .render_template(&self.text, &context)
            .map_err(|e| anyhow!("Template failed to render: {e}"))?;
        Ok((html, text))
    }

    /// Returns the handlebars struct that we use for rendering. It is set to `strict_mode` so
    /// missing variables will actually result in an error rather than in an empty field.
    fn renderer() -> handlebars::Handlebars<'static> {
        let mut hbs = handlebars::Handlebars::new();
        hbs.set_strict_mode(true);
        hbs
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{fs, io};

    #[test]
    fn test_parse_emails() -> io::Result<()> {
        for entry in fs::read_dir("mails")? {
            let path = entry?.path();
            let content = fs::read_to_string(&path)?;
            let res: Result<Templates, _> = toml::from_str(&content);
            if let Err(e) = res {
                panic!("Template at {path:?} failed to parse with error `{e}`");
            }
        }
        Ok(())
    }
}
