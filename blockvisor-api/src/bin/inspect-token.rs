use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::BufReader;

use anyhow::{bail, ensure, Context, Result};
use argh::FromArgs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::{Role, Roles};
use blockvisor_api::auth::token::refresh::{Encoded, Refresh};
use blockvisor_api::auth::token::{BearerToken, Cipher};
use blockvisor_api::config::token::SecretConfig;
use blockvisor_api::config::HumanTime;

const DEFAULT_TOKEN_EXPIRY: &str = "10m";

fn main() -> Result<()> {
    let args: Args = argh::from_env();

    let secret_config: SecretConfig = (&args).try_into()?;
    let cipher = Cipher::new(&secret_config);

    match args.command {
        Command::Validate(args) => validate(args, &cipher),
        Command::NewRoles(args) => new_roles(args, &cipher),
    }
}

fn validate(args: ValidateArgs, cipher: &Cipher) -> Result<()> {
    let config = BlockvisorConfig::read_file(&args.config)?;
    let decode_jwt = |token| {
        if args.expired {
            cipher.jwt.decode_expired(token)
        } else {
            cipher.jwt.decode(token)
        }
    };

    let token: BearerToken = config.token.into();
    let claims = decode_jwt(&token)?;
    eprintln!("valid token claims:\n{:#?}", claims);

    let encoded: Encoded = config.refresh_token.into();
    let _refresh = cipher.refresh.decode(&encoded)?;
    eprintln!("valid refresh token");

    Ok(())
}

fn new_roles(args: NewRolesArgs, cipher: &Cipher) -> Result<()> {
    let mut config = BlockvisorConfig::read_file(&args.config)?;
    let roles = args
        .role
        .iter()
        .map(|role| role.parse().or_else(|_| bail!("unknown role: {role}")))
        .collect::<Result<HashSet<Role>>>()?;

    let roles = match roles.len() {
        0 => bail!("at least one role is required in `--roles`"),
        1 => Roles::One(roles.into_iter().next().unwrap()),
        _ => Roles::Many(roles),
    };

    let token: BearerToken = config.token.into();
    let encoded: Encoded = config.refresh_token.into();

    let claims = cipher.jwt.decode_expired(&token)?;
    let refresh = cipher.refresh.decode(&encoded)?;
    let resource = claims.resource();
    ensure!(
        resource.id() == refresh.resource_id(),
        "claims resource does not match refresh"
    );

    let expires = args.expires.as_deref().unwrap_or(DEFAULT_TOKEN_EXPIRY);
    let expires: HumanTime = expires.parse().context("parse token expiry time")?;
    let expirable = Expirable::from_now(expires.try_into()?);

    let new_claims = if let Some(data) = claims.data {
        Claims::new(resource, expirable, roles.into()).with_data(data)
    } else {
        Claims::new(resource, expirable, roles.into())
    };

    let duration = refresh.expirable().duration();
    let new_refresh = Refresh::from_now(duration, resource);

    config.token = cipher.jwt.encode(&new_claims)?.to_string();
    config.refresh_token = cipher.refresh.encode(&new_refresh)?.to_string();

    config.write_file(&args.config)
}

/// `inspect-token` can inspect existing jwt tokens
#[derive(Debug, PartialEq, FromArgs)]
struct Args {
    /// the secret used to generate the JWT
    #[argh(option)]
    jwt_secret: String,
    /// the secret used to generate the fresh token
    #[argh(option)]
    refresh_secret: String,
    #[argh(subcommand)]
    command: Command,
}

impl TryFrom<&Args> for SecretConfig {
    type Error = anyhow::Error;

    fn try_from(args: &Args) -> std::result::Result<Self, Self::Error> {
        Ok(SecretConfig {
            jwt: args.jwt_secret.parse()?,
            refresh: args.refresh_secret.parse()?,
            jwt_fallback: Default::default(),
            refresh_fallback: Default::default(),
        })
    }
}

#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand)]
enum Command {
    Validate(ValidateArgs),
    NewRoles(NewRolesArgs),
}

/// validate an existing token config
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "validate")]
struct ValidateArgs {
    /// path to the token config
    #[argh(option, short = 'c')]
    config: String,
    /// allow expired tokens
    #[argh(switch, short = 'e')]
    expired: bool,
}

/// regenerate token config with a new set of roles
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "new-roles")]
struct NewRolesArgs {
    /// path to the token config
    #[argh(option, short = 'c')]
    config: String,
    /// new token expires in seconds
    #[argh(option, short = 'e')]
    expires: Option<String>,
    /// grant access to role
    #[argh(option, short = 'r')]
    role: Vec<String>,
}

/// A simplified representation of `Config` from `bv/src/config.rs`.
#[derive(Debug, Serialize, Deserialize)]
struct BlockvisorConfig {
    id: Uuid,
    token: String,
    refresh_token: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl BlockvisorConfig {
    pub fn read_file(file: &str) -> Result<Self> {
        let mut reader = File::open(file)
            .map(BufReader::new)
            .context("open config file")?;

        serde_json::from_reader(&mut reader).context("parse config file")
    }

    pub fn write_file(&self, file: &str) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(file)
            .context("open config file for writing")?;

        serde_json::to_writer(file, self).context("write new config file")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCKVISOR_JSON: &str = r#"{
        "id": "aa898cb3-4bb1-4a06-8698-7aec3fa4ebed",
        "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJyZXNvdXJjZV90eXBlIjoiSG9zdCIsInJlc291cmNlX2lkIjoiYWE4OThjYjMtNGJiMS00YTA2LTg2OTgtN2FlYzNmYTRlYmVkIiwiaWF0IjoxNjk1ODk2ODk5LCJleHAiOjE2OTU4OTc0OTksInJvbGVzIjoiZ3JwYy1sb2dpbiIsImRhdGEiOnt9fQ.2WNdJFa8CdrB_7esUM8OOf_GRVWJhJ4GtdGw7qitafKvaZA5zU6xgKFsjGhEQ8EES-gawGQt1k69bVdCNKvy3g",
        "refresh_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJyZXNvdXJjZV9pZCI6ImFhODk4Y2IzLTRiYjEtNGEwNi04Njk4LTdhZWMzZmE0ZWJlZCIsImlhdCI6MTY5NTg5Njg5OSwiZXhwIjoxNjk4NDg4ODk5fQ.X913aFvOiaADhnLDZtRUlsXxMM9whMTZJ62GtVVSnaq10aZcwc_xG28Ytji1MmHvFyvn8z3PWQ8tAgM2VbkdTw",
        "blockjoy_api_url": "https://api.dev.blockjoy.com",
        "blockjoy_mqtt_url": "mqtt://mqtt.dev.blockjoy.com:1883",
        "update_check_interval_secs": 60,
        "blockvisor_port": 9001,
        "iface": "bvbr0"
    }"#;

    #[test]
    fn can_parse_blockvisord_config() {
        let expected: Value = serde_json::from_str(BLOCKVISOR_JSON).unwrap();
        let expected = expected.as_object().unwrap();

        let config: BlockvisorConfig = serde_json::from_str(BLOCKVISOR_JSON).unwrap();
        let serialized = serde_json::to_string(&config).unwrap();

        let object: Value = serde_json::from_str(&serialized).unwrap();
        let object = object.as_object().unwrap();

        assert_eq!(object.len(), expected.len());
        assert_eq!(object.get("id").unwrap(), expected.get("id").unwrap());
        assert_eq!(object.get("iface").unwrap(), expected.get("iface").unwrap());
    }
}
