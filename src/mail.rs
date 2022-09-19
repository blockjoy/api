use anyhow::anyhow;
use sqlx::PgPool;

use crate::{auth, errors, models};
use std::collections::HashMap;

pub struct MailClient {
    client: sendgrid::SGClient,
}

impl MailClient {
    pub fn new() -> Self {
        let sg_api_key = dotenv::var("SENDGRID_API_KEY").expect("No `SENDGRID_API_KEY` provided!");
        Self {
            client: sendgrid::SGClient::new(sg_api_key),
        }
    }

    /// Sends a password reset email to the specified user, containing a JWT that they can use to
    /// authenticate themselves to reset their password.
    pub async fn reset_password(&self, user: &models::User, db: &PgPool) -> errors::Result<()> {
        use auth::TokenType::*;
        const TEMPLATES: &str = include_str!("../mails/reset_password.toml");
        // SAFETY: assume we can write toml and also protected by test
        let templates = toml::from_str(TEMPLATES).unwrap();
        let login_token = models::Token::get::<models::User>(user.id, Login, db).await?;
        let token = models::Token::create_for(user, login_token.role, PwdReset, db).await?;
        models::UserToken::new(user.id, token.id, PwdReset)
            .create_or_update(db)
            .await?;
        let mut context = HashMap::new();
        context.insert("token".to_owned(), token.token);
        self.send_mail(&templates, user, context).await
    }

    async fn send_mail(
        &self,
        templates: &Templates,
        to: &models::User,
        // Can't use 'static str for the keys or the values here, see:
        // https://stackoverflow.com/questions/68591843
        context: HashMap<String, String>,
    ) -> errors::Result<()> {
        let template = templates.by_lang(to.preferred_language());
        let (html, text) = template.render(context)?;

        let to = sendgrid::Destination {
            address: &to.email,
            name: &format!("{} {}", to.first_name, to.last_name),
        };
        let mail = sendgrid::Mail {
            to: vec![to],
            from: "hello@blockjoy.com",
            subject: "Password Reset",
            html: &html,
            text: &text,
            from_name: "BlockJoy",
            date: &chrono::Utc::now().to_rfc2822(),
            ..Default::default()
        };
        self.client
            .send(mail)
            .await
            .map_err(|e| anyhow!("Failure to send email {e}"))?;
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
            let content = fs::read_to_string(entry?.path())?;
            let _: Templates = toml::from_str(&content).unwrap();
        }
        Ok(())
    }
}
