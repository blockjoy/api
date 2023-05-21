use axum::http::header::AUTHORIZATION;
use axum::http::Request;
use blockvisor_api::auth::expiration_provider::ExpirationProvider;
use blockvisor_api::auth::token::*;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use std::str::FromStr;
use uuid::Uuid;

#[test]
fn should_encode_token() -> anyhow::Result<()> {
    let test_secret = "123456";
    temp_env::with_var("JWT_SECRET", Some(test_secret), || {
        let id = Uuid::new_v4();
        let claim = TokenClaim::new(id, 123123, TokenType::UserAuth, TokenRole::User, None);
        let token = UserAuthToken::try_new(claim)?;
        let header = Header::new(Algorithm::HS512);

        match jsonwebtoken::encode(
            &header,
            &token,
            &EncodingKey::from_secret(test_secret.as_ref()),
        ) {
            Ok(token_str) => assert_eq!(token_str, token.encode().unwrap()),
            Err(e) => panic!("encoding failed: {e}"),
        }

        Ok(())
    })
}

#[test]
fn should_decode_valid_token() -> anyhow::Result<()> {
    let test_secret = "123456";
    temp_env::with_var("JWT_SECRET", Some(test_secret), || {
        let id = Uuid::new_v4();
        let claim = TokenClaim::new(
            id,
            chrono::Utc::now().timestamp(),
            TokenType::UserAuth,
            TokenRole::User,
            None,
        );
        let token = UserAuthToken::try_new(claim)?;
        let token_str = token.encode().unwrap();
        let mut validation = Validation::new(Algorithm::HS512);

        validation.validate_exp = true;

        match jsonwebtoken::decode::<UserAuthToken>(
            token_str.as_str(),
            &DecodingKey::from_secret(test_secret.as_bytes()),
            &validation,
        ) {
            Ok(decoded_data) => assert_eq!(decoded_data.claims.id, id),
            Err(e) => panic!("decoding failed: {e}"),
        }

        Ok(())
    })
}

#[test]
fn should_panic_on_decode_expired_token() {
    let test_secret = "123456";
    temp_env::with_var("JWT_SECRET", Some(test_secret), || {
        let id = Uuid::new_v4();
        let claim = TokenClaim::new(id, 123123, TokenType::UserAuth, TokenRole::User, None);
        let token = UserAuthToken::try_new(claim).unwrap();
        let token_str = token.encode().unwrap();
        let mut validation = Validation::new(Algorithm::HS512);

        validation.validate_exp = true;

        match jsonwebtoken::decode::<UserAuthToken>(
            token_str.as_str(),
            &DecodingKey::from_secret(test_secret.as_bytes()),
            &validation,
        ) {
            Err(e) => assert_eq!(format!("{e}"), "ExpiredSignature"),
            _ => panic!("it worked, but it shouldn't"),
        };
    });
}

#[test]
#[should_panic]
fn should_panic_with_invalid_token() {
    let request = Request::builder()
        .uri("/")
        .method("GET")
        .header(AUTHORIZATION, "some-token")
        .body(())
        .unwrap();
    let _ = UserAuthToken::from_request(&request).unwrap();
}

#[test]
fn should_not_work_with_empty_token() {
    let request = Request::builder()
        .uri("/")
        .method("GET")
        .header(AUTHORIZATION, "")
        .body(())
        .unwrap();

    assert!(UserAuthToken::from_request(&request).is_err());
}

#[test]
fn should_get_valid_token() -> anyhow::Result<()> {
    let test_secret = "123456";
    temp_env::with_var("JWT_SECRET", Some(test_secret), || {
        let id = Uuid::new_v4();
        let claim = TokenClaim::new(
            id,
            ExpirationProvider::expiration(TokenType::UserAuth),
            TokenType::UserAuth,
            TokenRole::User,
            None,
        );
        let token = UserAuthToken::try_new(claim)?;
        let encoded = base64::encode(token.encode().unwrap());
        let request = Request::builder()
            .header(AUTHORIZATION, format!("Bearer {encoded}"))
            .uri("/")
            .method("GET")
            .body(())?;
        let token = UserAuthToken::from_request(&request).unwrap();
        assert_eq!(token.id, id);
        Ok(())
    })
}

#[test]
fn should_not_decode_without_secret_in_envs() {
    assert!(UserAuthToken::from_str("asf.asdfasdfasdfasdfsadfasdfasdf.asdfasfasdf").is_err());
}

#[test]
fn should_panic_on_encode_with_empty_secret_in_envs() {
    temp_env::with_var("JWT_SECRET", Some(""), || {
        let claim = TokenClaim::new(
            Uuid::new_v4(),
            123123123,
            TokenType::UserAuth,
            TokenRole::User,
            None,
        );
        let token = UserAuthToken::try_new(claim).unwrap();

        assert!(token.encode().is_err());
    });
}

#[test]
fn should_panic_on_decode_with_empty_secret_in_envs() {
    temp_env::with_var("JWT_SECRET", Some(""), || {
        assert!(UserAuthToken::from_str("asf.asdfasdfasdfasdfsadfasdfasdf.asdfasfasdf").is_err());
    });
}
