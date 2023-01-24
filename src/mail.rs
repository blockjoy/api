use crate::auth::key_provider::KeyProvider;
use crate::auth::{
    InvitationToken, JwtToken, PwdResetToken, RegistrationConfirmationToken, TokenRole, TokenType,
};
use crate::{errors, models};
use anyhow::anyhow;
use std::collections::HashMap;

pub struct MailClient {
    client: sendgrid::SGClient,
}

impl MailClient {
    pub fn new() -> Self {
        // Don't fail if API key wasn't found
        let sg_api_key = match KeyProvider::get_var("SENDGRID_API_KEY") {
            Ok(key) => key.to_string(),
            Err(e) => {
                tracing::error!("Couldn't read SENDGRID_API_KEY env var: {}", e);
                String::default()
            }
        };

        Self {
            client: sendgrid::SGClient::new(sg_api_key),
        }
    }

    /// Sends a notification if the user has updated his password
    pub async fn update_password(&self, user: &models::User) -> errors::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/update_password.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;

        self.send_mail(&templates, user, None).await
    }

    pub async fn registration_confirmation(&self, user: &models::User) -> errors::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/register.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;
        let confirmation_token = RegistrationConfirmationToken::create_token_for(
            user,
            TokenType::RegistrationConfirmation,
            TokenRole::User,
            None,
        )?
        .encode()?;
        let base_url =
            dotenv::var("UI_BASE_URL").map_err(|e| anyhow!("UI_BASE_URL can't be read: {e}"))?;
        let link = format!("{}/verified?token={}", base_url, confirmation_token);
        let mut context = HashMap::new();
        context.insert("link".to_owned(), link);

        self.send_mail(&templates, user, Some(context)).await
    }

    pub async fn invitation_for_registered(
        &self,
        inviter: &models::User,
        invitee: &models::User,
        expiration: String,
    ) -> errors::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/invite_registered_user.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;
        let base_url =
            dotenv::var("UI_BASE_URL").map_err(|e| anyhow!("UI_BASE_URL can't be read: {e}"))?;
        let link = format!("{}/invite-registered?uid={}", base_url, invitee.id);
        let inviter = format!(
            "{} {} ({})",
            inviter.first_name, inviter.last_name, inviter.email
        );
        let mut context = HashMap::new();
        context.insert("inviter".to_owned(), inviter);
        context.insert("link".to_owned(), link);
        context.insert("expiration".to_owned(), expiration);

        self.send_mail(&templates, invitee, Some(context)).await
    }

    pub async fn invitation(
        &self,
        invitation: &models::Invitation,
        inviter: &models::User,
        invitee: &models::User,
        expiration: String,
    ) -> errors::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/invite_user.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;
        let confirmation_token = InvitationToken::create_for_invitation(invitation)?.encode()?;
        let base_url =
            dotenv::var("UI_BASE_URL").map_err(|e| anyhow!("UI_BASE_URL can't be read: {e}"))?;
        let accept_link = format!("{}/accept-invite?token={}", base_url, confirmation_token);
        let decline_link = format!("{}/decline-invite?token={}", base_url, confirmation_token);
        let inviter = format!(
            "{} {} ({})",
            inviter.first_name, inviter.last_name, inviter.email
        );
        let mut context = HashMap::new();
        context.insert("inviter".to_owned(), inviter);
        context.insert("accept_link".to_owned(), accept_link);
        context.insert("decline_link".to_owned(), decline_link);
        context.insert("expiration".to_owned(), expiration);

        self.send_mail(&templates, invitee, Some(context)).await
    }

    /// Sends a password reset email to the specified user, containing a JWT that they can use to
    /// authenticate themselves to reset their password.
    pub async fn reset_password(
        &self,
        user: &models::User,
        _db: &mut sqlx::PgConnection,
    ) -> errors::Result<()> {
        const TEMPLATES: &str = include_str!("../mails/reset_password.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES)
            .map_err(|e| anyhow!("Our email toml template {TEMPLATES} is bad! {e}"))?;
        let token: PwdResetToken = JwtToken::create_token_for::<models::User>(
            user,
            TokenType::PwdReset,
            TokenRole::User,
            None,
        )?;
        let base_url =
            dotenv::var("UI_BASE_URL").map_err(|e| anyhow!("UI_BASE_URL can't be read: {e}"))?;
        let link = format!("{}/password_reset?token={}", base_url, token.encode()?);
        let mut context = HashMap::new();
        context.insert("link".to_owned(), link);

        self.send_mail(&templates, user, Some(context)).await
    }

    async fn send_mail(
        &self,
        templates: &Templates,
        to: &models::User,
        // Can't use 'static str for the keys or the values here, see:
        // https://stackoverflow.com/questions/68591843
        context: Option<HashMap<String, String>>,
    ) -> errors::Result<()> {
        let context = context.unwrap_or_default();
        let template = templates.by_lang(to.preferred_language());
        let (html, text) = template.render(context)?;

        let to = sendgrid::Destination {
            address: &to.email,
            name: &format!("{} {}", to.first_name, to.last_name),
        };
        let mail = sendgrid::Mail {
            to: vec![to],
            from: "no-reply@blockjoy.com",
            subject: "A message from BlockJoy",
            html: &html,
            text: &text,
            from_name: "BlockJoy",
            date: &chrono::Utc::now().to_rfc2822(),
            ..Default::default()
        };

        // Don't fail if mail couldn't be sent
        if let Err(e) = self.client.send(mail).await {
            tracing::error!("Failure to send email {e}");
        }

        Ok(())
    }
}

impl Default for MailClient {
    fn default() -> Self {
        Self::new()
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
    fn render(&self, context: HashMap<String, String>) -> errors::Result<(String, String)> {
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
