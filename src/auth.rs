use crate::errors::{ApiError, Result};
use anyhow::anyhow;
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use sqlx::PgPool;
use uuid::Uuid;

const JWT_SECRET: &str = "?G'A$jNW<$6x(PdFP?4VdRvmotIV^^";

#[tonic::async_trait]
pub trait FindableByToken: Send + Sync + 'static {
    async fn find_by_token(token: &str, db: &PgPool) -> Result<Self>
        where Self: Sized;
}

pub struct AuthData {
    pub user_id: uuid::Uuid,
    pub user_role: String,
}

pub enum JwtValidationStatus {
    Valid(AuthData),
    Expired(AuthData),
    Invalid,
}

#[derive(Debug, Deserialize, Serialize)]
struct Claims {
    sub: String,
    role: String,
    exp: usize,
}

/// Creates a jwt with default duration for expires.
pub fn create_jwt(data: &AuthData) -> Result<String> {
    let duration = chrono::Duration::days(1);
    create_jwt_with_duration(data, duration)
}

pub fn create_temp_jwt(data: &AuthData) -> Result<String> {
    let duration = chrono::Duration::seconds(60 * 12);
    create_jwt_with_duration(data, duration)
}

pub fn create_jwt_with_duration(data: &AuthData, duration: chrono::Duration) -> Result<String> {
    let exp = Utc::now()
        .checked_add_signed(duration)
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: data.user_id.to_string(),
        role: data.user_role.clone(),
        exp: exp as usize,
    };

    let header = Header::new(Algorithm::HS512);
    Ok(encode(
        &header,
        &claims,
        &EncodingKey::from_secret(jwt_secret().as_bytes()),
    )?)
}

pub fn validate_jwt(jwt: &str) -> Result<JwtValidationStatus> {
    let validation = Validation {
        leeway: 60,
        validate_exp: false,
        algorithms: vec![Algorithm::HS512],
        ..Default::default()
    };

    let result = match decode::<Claims>(
        jwt,
        &DecodingKey::from_secret(jwt_secret().as_bytes()),
        &validation,
    ) {
        Ok(decoded) => {
            let user_id = Uuid::parse_str(&decoded.claims.sub)
                .map_err(|_| ApiError::from(anyhow!("Error parsing uuid from JWT sub")))?;
            let user_role = decoded.claims.role;

            let auth_data = AuthData { user_id, user_role };

            // Check Expiration
            let exp = decoded.claims.exp;
            let now = get_current_timestamp();
            if (exp as u64) < now - 60 {
                JwtValidationStatus::Expired(auth_data)
            } else {
                JwtValidationStatus::Valid(auth_data)
            }
        }

        Err(_) => JwtValidationStatus::Invalid,
    };

    Ok(result)
}

fn jwt_secret() -> String {
    env::var("JWT_SECRET").unwrap_or_else(|_| JWT_SECRET.to_string())
}

fn get_current_timestamp() -> u64 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}
