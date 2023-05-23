pub mod expiration_provider;
pub mod key_provider;
mod token;

use diesel_async::AsyncPgConnection;
pub use token::*;

pub const TOKEN_EXPIRATION_MINS: &str = "TOKEN_EXPIRATION_MINS";
pub const REFRESH_EXPIRATION_USER_MINS: &str = "REFRESH_EXPIRATION_USER_MINS";
pub const REFRESH_EXPIRATION_HOST_MINS: &str = "REFRESH_EXPIRATION_HOST_MINS";

/// This function is the workhorse of our authentication process. It takes the extensions of the
/// request and returns the `Claims` that the authentication process provided.
pub async fn get_claims<T>(
    req: &tonic::Request<T>,
    endpoint: Endpoint,
    conn: &mut AsyncPgConnection,
) -> crate::Result<Claims> {
    let meta = req
        .metadata()
        .get("authorization")
        .ok_or_else(|| crate::Error::invalid_auth("No JWT or API key"))?
        .to_str()?;
    let claims = if let Ok(claims) = claims_from_jwt(meta) {
        claims
    } else if let Ok(claims) = claims_from_api_key(meta, conn).await {
        claims
    } else {
        let msg = "Neither JWT nor API key are valid";
        return Err(crate::Error::invalid_auth(msg));
    };

    if !claims.endpoints.includes(endpoint) {
        return Err(crate::Error::invalid_auth("No access to this endpoint"));
    }

    Ok(claims)
}

fn claims_from_jwt(meta: &str) -> crate::Result<Claims> {
    const ERROR_MSG: &str = "Authorization meta must start with `Bearer `";
    let stripped = meta
        .strip_prefix("Bearer ")
        .ok_or_else(|| crate::Error::invalid_auth(ERROR_MSG))?;
    let jwt = Jwt::decode(stripped)?;
    Ok(jwt.claims)
}

async fn claims_from_api_key(_meta: &str, _conn: &mut AsyncPgConnection) -> crate::Result<Claims> {
    Err(crate::Error::unexpected("Chris will implement this"))
}

pub fn get_refresh<T>(req: &tonic::Request<T>) -> crate::Result<Option<Refresh>> {
    let meta = match req.metadata().get("cookie") {
        Some(meta) => meta.to_str()?,
        None => return Ok(None),
    };
    let Some(refresh_idx) = meta.find("refresh=") else { return Ok(None) };
    let end_idx = meta[refresh_idx..]
        .find(';')
        .map(|offset| offset + refresh_idx)
        .unwrap_or(meta.len());
    if refresh_idx > end_idx {
        return Ok(None);
    };
    // Note that `refresh + 8` can never cause an out of bounds access, because we found the string
    // `"refresh="` and then `";"` after that, so there must be at least 10 characters occuring
    // after `refresh_idx`
    let refresh = Refresh::decode(&meta[refresh_idx + 8..end_idx])?;
    Ok(Some(refresh))
}

#[cfg(test)]
mod tests {
    use crate::auth;

    use super::*;

    #[test]
    fn test_get_refresh() {
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let refresh_exp =
                expiration_provider::ExpirationProvider::expiration(REFRESH_EXPIRATION_USER_MINS)
                    .unwrap();
            let token =
                Refresh::new(uuid::Uuid::new_v4(), chrono::Utc::now(), refresh_exp).unwrap();
            let mut req = tonic::Request::new(());
            req.metadata_mut()
                .insert("cookie", token.as_set_cookie().unwrap().parse().unwrap());
            let res = get_refresh(&req).unwrap().unwrap();
            assert_eq!(token.resource_id, res.resource_id);
        });
    }

    #[tokio::test]
    async fn test_crafted_evil_refresh() {
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let mut req = tonic::Request::new(());

            req.metadata_mut()
                .insert("cookie", ";refresh=".parse().unwrap());
            assert!(get_refresh(&req).is_err());

            req.metadata_mut()
                .insert("cookie", "refresh=;".parse().unwrap());
            assert!(get_refresh(&req).is_err());
        });
    }

    #[tokio::test]
    async fn test_extra_cookies() {
        let db = crate::TestDb::setup().await;
        let iat = chrono::Utc::now();
        let exp = chrono::Duration::seconds(65);
        let refresh = auth::Refresh::new(db.user().await.id, iat, exp).unwrap();
        let refresh = refresh.encode().unwrap();
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let mut req = tonic::Request::new(());

            req.metadata_mut().insert(
                "cookie",
                format!("other_meta=v1; refresh={refresh}; another=v2; ")
                    .parse()
                    .unwrap(),
            );
            get_refresh(&req).unwrap().unwrap();

            req.metadata_mut().insert(
                "cookie",
                format!("other_meta=v1; refresh={refresh}").parse().unwrap(),
            );
            get_refresh(&req).unwrap().unwrap();

            req.metadata_mut()
                .insert("cookie", format!("refresh={refresh}").parse().unwrap());
            get_refresh(&req).unwrap().unwrap();
        });
    }
}
