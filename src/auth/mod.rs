pub mod token;

use token::refresh::Refresh;
use token::{Claims, Endpoint};

use crate::config::Context;
use crate::models::Conn;

pub const TOKEN_EXPIRATION_MINS: &str = "TOKEN_EXPIRATION_MINS";
pub const REFRESH_EXPIRATION_USER_MINS: &str = "REFRESH_EXPIRATION_USER_MINS";
pub const REFRESH_EXPIRATION_HOST_MINS: &str = "REFRESH_EXPIRATION_HOST_MINS";

/// This function is the workhorse of our authentication process. It takes the extensions of the
/// request and returns the `Claims` that the authentication process provided.
pub async fn get_claims<T>(
    req: &tonic::Request<T>,
    endpoint: Endpoint,
    conn: &mut Conn,
) -> crate::Result<Claims> {
    let meta = req
        .metadata()
        .get("authorization")
        .ok_or_else(|| crate::Error::invalid_auth("No JWT or API key"))?
        .to_str()?;

    let claims = match (
        claims_from_jwt(meta, conn),
        claims_from_api_key(meta, conn).await,
    ) {
        (Ok(claims), _) => claims,
        (_, Ok(claims)) => claims,
        (Err(e1), Err(e2)) => {
            let msg = format!("Neither JWT nor API key are valid: `{e1}` and `{e2}`");
            return Err(crate::Error::invalid_auth(msg));
        }
    };

    if !claims.endpoints.includes(endpoint) {
        return Err(crate::Error::invalid_auth("No access to this endpoint"));
    }

    Ok(claims)
}

fn claims_from_jwt(meta: &str, conn: &Conn) -> crate::Result<Claims> {
    const ERROR_MSG: &str = "Authorization meta must start with `Bearer `";
    let stripped = meta
        .strip_prefix("Bearer ")
        .ok_or_else(|| crate::Error::invalid_auth(ERROR_MSG))?;

    conn.context.cipher.jwt.decode(stripped).map_err(Into::into)
}

async fn claims_from_api_key(_meta: &str, _conn: &mut Conn) -> crate::Result<Claims> {
    Err(crate::Error::unexpected("Chris will implement this"))
}

pub fn get_refresh<T>(
    req: &tonic::Request<T>,
    context: &Context,
) -> crate::Result<Option<Refresh>> {
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
    let refresh = context
        .cipher
        .refresh
        .decode(&meta[refresh_idx + 8..end_idx])?;

    Ok(Some(refresh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Context;
    use crate::{auth, TestDb};

    #[test]
    fn test_get_refresh() {
        let context = Context::new_with_default_toml().unwrap();

        let refresh = Refresh::new(
            uuid::Uuid::new_v4(),
            chrono::Utc::now(),
            chrono::Duration::minutes(1),
        )
        .unwrap();

        let mut req = tonic::Request::new(());
        req.metadata_mut()
            .insert("cookie", context.cipher.refresh.cookie(&refresh).unwrap());

        let res = get_refresh(&req, &context).unwrap().unwrap();
        assert_eq!(res.resource_id, refresh.resource_id);
    }

    #[tokio::test]
    async fn test_crafted_evil_refresh() {
        let context = Context::new_with_default_toml().unwrap();
        let mut req = tonic::Request::new(());

        req.metadata_mut()
            .insert("cookie", ";refresh=".parse().unwrap());
        assert!(get_refresh(&req, &context).is_err());

        req.metadata_mut()
            .insert("cookie", "refresh=;".parse().unwrap());
        assert!(get_refresh(&req, &context).is_err());
    }

    #[tokio::test]
    async fn test_extra_cookies() {
        let context = Context::new_with_default_toml().unwrap();
        let db = TestDb::setup(context.clone()).await;

        let iat = chrono::Utc::now();
        let exp = chrono::Duration::seconds(65);
        let refresh = auth::Refresh::new(db.user().await.id, iat, exp).unwrap();

        let refresh = context.cipher.refresh.encode(&refresh).unwrap();

        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "cookie",
            format!("other_meta=v1; refresh={}; another=v2; ", *refresh)
                .parse()
                .unwrap(),
        );
        get_refresh(&req, &context).unwrap().unwrap();

        req.metadata_mut().insert(
            "cookie",
            format!("other_meta=v1; refresh={}", *refresh)
                .parse()
                .unwrap(),
        );
        get_refresh(&req, &context).unwrap().unwrap();

        req.metadata_mut()
            .insert("cookie", format!("refresh={}", *refresh).parse().unwrap());
        get_refresh(&req, &context).unwrap().unwrap();
    }
}
