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
    let meta = match req.metadata().get("Cookie") {
        Some(meta) => meta.to_str()?,
        None => return Ok(None),
    };
    let Some(refresh_idx) = meta.find("refresh=") else { return Ok(None) };
    let Some(end_idx) = meta.find("; ") else { return Ok(None) };
    let refresh = Refresh::decode(&meta[refresh_idx + 8..end_idx])?;
    Ok(Some(refresh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_refresh() {
        temp_env::with_var_unset("SECRETS_ROOT", || {
            let iat = chrono::Utc::now();
            let refresh_exp =
                expiration_provider::ExpirationProvider::expiration(REFRESH_EXPIRATION_USER_MINS)
                    .unwrap();
            let token =
                Refresh::new(uuid::Uuid::new_v4(), chrono::Utc::now(), refresh_exp).unwrap();
            let mut req = tonic::Request::new(());
            req.metadata_mut()
                .insert("cookie", token.as_set_cookie(iat).unwrap().parse().unwrap());
            let res = get_refresh(&req).unwrap().unwrap();
            assert_eq!(token.resource_id, res.resource_id);
        });
    }
}
