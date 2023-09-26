use std::collections::HashMap;

use derive_more::Deref;
use displaydoc::Display;
use handlebars::{Context, Handlebars};
use serde::Deserialize;
use thiserror::Error;

const INVITE_USER: &str = include_str!("../../mails/invite_user.toml");
const INVITE_REGISTERED: &str = include_str!("../../mails/invite_registered_user.toml");
const REGISTRATION_CONFIRMATION: &str = include_str!("../../mails/register.toml");
const RESET_PASSWORD: &str = include_str!("../../mails/reset_password.toml");
const UPDATE_PASSWORD: &str = include_str!("../../mails/update_password.toml");

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build handlebars::Context: {0}
    Context(handlebars::RenderError),
    /// Missing default English template: {0:?}
    MissingEnglish(Kind),
    /// Template not found: {0:?}
    NoTemplate(Kind),
    /// Failed to parse toml template for {0:?}: {1}
    ParseTemplate(Kind, toml::de::Error),
    /// Failed to render HTML: {0}
    RenderHtml(handlebars::RenderError),
    /// Failed to render text: {0}
    RenderText(handlebars::RenderError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    En,
    De,
    Nl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    InviteUser,
    InviteRegistered,
    RegistrationConfirmation,
    ResetPassword,
    UpdatePassword,
}

impl Kind {
    pub fn subject(self) -> &'static str {
        match self {
            Kind::InviteUser => "[BlockJoy] Organization Invite",
            Kind::InviteRegistered => "[BlockJoy] Organization Invite",
            Kind::RegistrationConfirmation => "[BlockJoy] Verify Your Account",
            Kind::ResetPassword => "[BlockJoy] Reset Password",
            Kind::UpdatePassword => "[BlockJoy] Password Updated",
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct Template {
    pub html: String,
    pub text: String,
}

#[derive(Deref)]
struct Languages(HashMap<Language, Template>);

pub struct Templates {
    renderer: Handlebars<'static>,
    templates: HashMap<Kind, Languages>,
}

impl Templates {
    pub fn new() -> Result<Self, Error> {
        let kinds = [
            (Kind::InviteUser, INVITE_USER),
            (Kind::InviteRegistered, INVITE_REGISTERED),
            (Kind::RegistrationConfirmation, REGISTRATION_CONFIRMATION),
            (Kind::ResetPassword, RESET_PASSWORD),
            (Kind::UpdatePassword, UPDATE_PASSWORD),
        ];

        let mut templates = HashMap::new();
        for (kind, template) in kinds {
            let languages: HashMap<Language, Template> =
                toml::from_str(template).map_err(|err| Error::ParseTemplate(kind, err))?;
            let _ = languages
                .get(&Language::En)
                .ok_or(Error::MissingEnglish(kind))?;

            templates.insert(kind, Languages(languages));
        }

        let mut renderer = Handlebars::new();
        renderer.set_strict_mode(true);

        Ok(Templates {
            renderer,
            templates,
        })
    }

    /// Render a template by `Kind` and `Language`.
    ///
    /// If `language` is not found it falls back to the English template.
    pub fn render(
        &self,
        kind: Kind,
        language: Language,
        context: Option<HashMap<&'static str, String>>,
    ) -> Result<Template, Error> {
        let langs = self.templates.get(&kind).ok_or(Error::NoTemplate(kind))?;
        let template = langs
            .get(&language)
            .or_else(|| langs.get(&Language::En))
            .ok_or(Error::MissingEnglish(kind))?;
        let context = match context {
            Some(context) => Context::wraps(context).map_err(Error::Context)?,
            None => Context::null(),
        };

        let html = self
            .renderer
            .render_template_with_context(&template.html, &context)
            .map_err(Error::RenderHtml)?;
        let text = self
            .renderer
            .render_template_with_context(&template.text, &context)
            .map_err(Error::RenderText)?;

        Ok(Template { html, text })
    }
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    use crate::auth::Auth;
    use crate::config::Config;
    use crate::email::tests::MockEmail;
    use crate::email::{Email, Recipient};
    use crate::models::{Invitation, User};

    use super::*;

    #[tokio::test]
    async fn test_render_emails() {
        let config = Config::new().unwrap();
        let auth = Auth::new(&config.token);

        let email = Email {
            sender: Box::new(MockEmail {}),
            templates: Templates::new().unwrap(),
            cipher: auth.cipher,
            base_url: config.mail.ui_base_url.clone(),
            expires: config.token.expire,
        };
        let user = User {
            id: Uuid::new_v4().into(),
            email: "tmp@tmp.tmp".to_string(),
            hashword: "something fake".to_string(),
            salt: "something even faker".to_string(),
            created_at: Default::default(),
            first_name: "Luuk".to_string(),
            last_name: "Tester".to_string(),
            confirmed_at: Default::default(),
            deleted_at: Default::default(),
            billing_id: Default::default(),
        };
        let user2 = User {
            email: "testing@receiver.blockjoy".to_string(),
            first_name: "Shaun".to_string(),
            last_name: "Testheri".to_string(),
            ..user.clone()
        };
        let recipient = Recipient::from(&user2);
        let invitation = Invitation {
            id: Uuid::new_v4().into(),
            created_by: Uuid::new_v4().into(),
            org_id: Uuid::new_v4().into(),
            invitee_email: "testing@receiver.blockjoy".to_string(),
            created_at: Default::default(),
            accepted_at: Default::default(),
            declined_at: Default::default(),
        };

        email.update_password(&user).await.unwrap();
        email.registration_confirmation(&user, None).await.unwrap();
        email
            .invitation_for_registered(&invitation, &user, &user2, "tomorrow")
            .await
            .unwrap();
        email
            .invitation(&invitation, &user, recipient, "yesterday")
            .await
            .unwrap();
        email.reset_password(&user).await.unwrap();
    }
}
