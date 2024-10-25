use axum::response::IntoResponse;

use crate::database;

pub mod api_key;
pub mod chargebee;
pub mod health;
pub mod mqtt;
pub mod stripe;

struct Error {
    inner: ErrorInner,
    status: hyper::StatusCode,
}

impl Error {
    fn new(message: &str, status: u16) -> Self {
        Self {
            inner: ErrorInner {
                message: message.to_string(),
            },
            status: hyper::StatusCode::from_u16(status).unwrap(),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (self.status, axum::Json(self.inner)).into_response()
    }
}

impl From<database::Error> for Error {
    fn from(err: database::Error) -> Self {
        tracing::error!("{err}");
        Self {
            inner: ErrorInner {
                message: err.to_string(),
            },
            status: hyper::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<crate::auth::Error> for Error {
    fn from(value: crate::auth::Error) -> Self {
        use crate::auth::Error::*;
        let (message, status) = match value {
            Database(_) => ("Internal error.", 500),
            DecodeJwt(_) => ("Invalid JWT token.", 403),
            DecodeRefresh(_) | RefreshHeader(_) => ("Invalid refresh token.", 403),
            ExpiredJwt(_) => ("Jwt token expired", 401),
            ExpiredRefresh(_) => ("Refresh token expired", 401),
            ValidateApiKey(_) => ("Invalid API key.", 403),
            Claims(_err) => todo!(),
            ParseRequestToken(_err) => todo!(),
        };
        Self::new(message, status)
    }
}

impl From<crate::auth::claims::Error> for Error {
    fn from(err: crate::auth::claims::Error) -> Self {
        use crate::auth::claims::Error::*;
        let (message, status) = match err {
            EnsureHost(..) | EnsureNode(..) | EnsureOrg(..) | EnsureUser(..) => {
                ("Access denied.", 401)
            }
            MissingPerm(perm, _) => return Self::new(&format!("Missing permission: {perm}"), 401),
            Host(_err) => todo!(),
            Node(_err) => todo!(),
            Org(_err) => todo!(),
            Rbac(_err) => todo!(),
            User(_err) => todo!(),
        };
        Self::new(message, status)
    }
}

#[derive(serde::Serialize)]
struct ErrorInner {
    message: String,
}
