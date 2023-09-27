use std::collections::HashSet;

use anyhow::{bail, ensure, Result};
use argh::FromArgs;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::endpoint::Endpoints;
use blockvisor_api::auth::rbac::{Access, Perms, Role, Roles};
use blockvisor_api::auth::token::Cipher;
use blockvisor_api::config::token::SecretConfig;

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
    ensure!(
        args.jwt_token.is_some() || args.refresh_token.is_some(),
        "either `--jwt-token` or `--refresh-token` must be provided"
    );

    if let Some(token) = args.jwt_token {
        let claims = if args.expired {
            cipher.jwt.decode_expired(&(token.into()))
        } else {
            cipher.jwt.decode(&(token.into()))
        }?;
        print_access(&claims.access)
    }

    if let Some(token) = args.refresh_token {
        let _refresh = cipher.refresh.decode(&(token.into()))?;
        eprintln!("valid refresh token")
    }

    Ok(())
}

fn new_roles(args: NewRolesArgs, cipher: &Cipher) -> Result<()> {
    let roles = args
        .roles
        .iter()
        .map(|role| role.parse().or_else(|_| bail!("unknown role: {role}")))
        .collect::<Result<HashSet<Role>>>()?;

    let roles = match roles.len() {
        0 => bail!("at least one role is required in `--roles`"),
        1 => Roles::One(roles.into_iter().next().unwrap()),
        _ => Roles::Many(roles),
    };

    let claims = cipher.jwt.decode_expired(&(args.jwt_token.into()))?;
    let refresh = cipher.refresh.decode(&(args.refresh_token.into()))?;

    let expires = refresh.expirable().duration();
    let expirable = Expirable::from_now(expires);
    let resource = claims.resource();

    let new_claims = if let Some(data) = claims.data {
        Claims::new(resource, expirable, roles.into()).with_data(data)
    } else {
        Claims::new(resource, expirable, roles.into())
    };

    let new_token = cipher.jwt.encode(&new_claims)?;
    eprintln!("your new token is:\n{}", *new_token);

    Ok(())
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
        })
    }
}

#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand)]
enum Command {
    Validate(ValidateArgs),
    NewRoles(NewRolesArgs),
}

/// validate an existing token
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "validate")]
struct ValidateArgs {
    /// allow expired tokens
    #[argh(switch)]
    expired: bool,
    /// the JWT token to validate
    #[argh(option)]
    jwt_token: Option<String>,
    /// the refresh token to validate
    #[argh(option)]
    refresh_token: Option<String>,
}

/// regenerate token with a new set of roles
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "new-roles")]
struct NewRolesArgs {
    /// the existing jwt token with old roles
    #[argh(option)]
    jwt_token: String,
    /// the existing refresh token
    #[argh(option)]
    refresh_token: String,
    /// grant access to role
    #[argh(option)]
    roles: Vec<String>,
}

fn print_access(access: &Access) {
    match access {
        Access::Roles(Roles::One(role)) => eprintln!("valid access for role: {role}"),
        Access::Roles(Roles::Many(roles)) => {
            eprintln!("valid access for the following roles:");
            roles.iter().for_each(|role| eprintln!("\t{role}"));
        }

        Access::Perms(Perms::One(perm)) => eprintln!("valid access for permission: {perm}"),
        Access::Perms(Perms::Many(perms)) => {
            eprintln!("valid access for the following permissions:");
            perms.iter().for_each(|perm| eprintln!("\t{perm}"));
        }

        Access::Endpoints(Endpoints::Single(endpoint)) => {
            eprintln!("valid access for endpoint: {endpoint:?}");
        }

        Access::Endpoints(Endpoints::Multiple(endpoints)) => {
            eprintln!("valid access for the following endpoints:");
            endpoints
                .iter()
                .for_each(|endpoint| eprintln!("\t{endpoint:?}"));
        }
    }
}
