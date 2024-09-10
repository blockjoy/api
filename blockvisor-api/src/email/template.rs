use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use derive_more::Deref;
use displaydoc::Display;
use handlebars::{Context, Handlebars};
use serde::Deserialize;
use thiserror::Error;

const INVITE_USER: &str = "invite_user.toml";
const INVITE_REGISTERED: &str = "invite_registered_user.toml";
const REGISTRATION_CONFIRMATION: &str = "register.toml";
const RESET_PASSWORD: &str = "reset_password.toml";
const UPDATE_PASSWORD: &str = "update_password.toml";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build handlebars::Context: {0}
    Context(handlebars::RenderError),
    /// Missing default English template: {0:?}
    MissingEnglish(Kind),
    /// Template not found: {0:?}
    NoTemplate(Kind),
    /// Template directory not found: {0:?}
    NoTemplateDir(PathBuf),
    /// Template file not found: {0:?}
    NoTemplateFile(PathBuf),
    /// Failed to parse toml template for {0:?}: {1}
    ParseTemplate(Kind, toml::de::Error),
    /// Failed to read toml template for {0:?}: {1}
    ReadTemplate(Kind, std::io::Error),
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
    pub const fn subject(self) -> &'static str {
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
    pub fn new<P>(template_dir: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let path = template_dir.as_ref();
        if !path.is_dir() {
            return Err(Error::NoTemplateDir(path.to_path_buf()));
        }

        let kinds = [
            (Kind::InviteUser, INVITE_USER),
            (Kind::InviteRegistered, INVITE_REGISTERED),
            (Kind::RegistrationConfirmation, REGISTRATION_CONFIRMATION),
            (Kind::ResetPassword, RESET_PASSWORD),
            (Kind::UpdatePassword, UPDATE_PASSWORD),
        ];

        let templates = kinds
            .into_iter()
            .map(|(kind, file)| {
                let path = path.join(file);
                if !path.is_file() {
                    return Err(Error::NoTemplateFile(path));
                }

                let contents =
                    fs::read_to_string(path).map_err(|err| Error::ReadTemplate(kind, err))?;
                let languages: HashMap<Language, Template> =
                    toml::from_str(&contents).map_err(|err| Error::ParseTemplate(kind, err))?;

                let _ = languages
                    .get(&Language::En)
                    .ok_or(Error::MissingEnglish(kind))?;

                Ok((kind, Languages(languages)))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

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
    use chrono::DateTime;
    use uuid::Uuid;

    use crate::auth::resource::ResourceType;
    use crate::auth::Auth;
    use crate::config::Config;
    use crate::email::tests::MockEmail;
    use crate::email::{Email, Recipient};
    use crate::model::{Invitation, User};

    use super::*;

    #[tokio::test]
    async fn test_render_emails() {
        let config = Config::new().unwrap();
        let auth = Auth::new(&config.token);
        let email = Email {
            sender: Box::new(MockEmail {}),
            templates: Templates::new(&config.email.template_dir).unwrap(),
            cipher: auth.cipher,
            base_url: config.email.ui_base_url.clone(),
            expires: config.token.expire,
        };

        let user = User {
            id: Uuid::new_v4().into(),
            email: "tmp@tmp.tmp".to_string(),
            hashword: "something fake".to_string(),
            salt: "something even faker".to_string(),
            created_at: DateTime::default(),
            first_name: "Luuk".to_string(),
            last_name: "Tester".to_string(),
            confirmed_at: None,
            deleted_at: None,
        };
        let recipient = Recipient::from(&user);
        let invitation = Invitation {
            id: Uuid::new_v4().into(),
            org_id: Uuid::new_v4().into(),
            invitee_email: "testing@receiver.blockjoy".to_string(),
            invited_by: Uuid::new_v4().into(),
            invited_by_resource: ResourceType::User,
            created_at: DateTime::default(),
            accepted_at: None,
            declined_at: None,
        };
        let inviter = "Mahatma Gandhi".to_string();

        email.update_password(&user).await.unwrap();
        email.registration_confirmation(&user, None).await.unwrap();
        email
            .invitation_for_registered(&invitation, inviter.clone(), &user, "tomorrow")
            .await
            .unwrap();
        email
            .invitation(&invitation, inviter, recipient, "yesterday")
            .await
            .unwrap();
        email.reset_password(&user).await.unwrap();
    }
}
