//! Actual authorization happens here

use casbin::prelude::*;
use std::env;
use std::env::VarError;
use thiserror::Error;

pub type AuthorizationResult = std::result::Result<AuthorizationState, AuthorizationError>;
pub type InitResult = std::result::Result<Authorization, AuthorizationError>;

/// Restrict possible authorization results
pub enum AuthorizationState {
    Authorized,
    Denied,
}

/// Restrict possible ownership states
pub enum OwnershipState {
    Owned,
    NotOwned
}

#[derive(Error, Debug)]
pub enum AuthorizationError {
    #[error("Generic Casbin Error: `{0:?}`")]
    CasbinError(#[from] casbin::error::Error),

    #[error("Insufficient privileges error: `{0:?}`")]
    InsufficientPriviliges(#[from] casbin::error::PolicyError),

    #[error("Malformed request error: `{0:?}`")]
    MalformedRequest(#[from] casbin::error::RequestError),

    #[error("Malformed or missing env vars error: `{0:?}`")]
    MissingEnv(#[from] VarError),
}

/// Authorization namespace
/// Implements a simple ACL based authorization solution.
/// Users must belong to a group, the authorization will be tested
/// against that group
pub struct Authorization {
    enforcer: Enforcer,
}

impl Authorization {
    /// Creates a new Authorization object using configuration as defined in
    /// env vars ***CASBIN_MODEL*** and ***CASBIN_POLICIES***
    pub async fn new() -> InitResult {
        let model = env::var("CASBIN_MODEL").expect("Couldn't load auth model");
        let policies = env::var("CASBIN_POLICIES").expect("Couldn't load auth policies");

        match Enforcer::new(
            Authorization::string_to_static_str(model),
            Authorization::string_to_static_str(policies),
        )
        .await
        {
            Ok(enforcer) => Ok(Self { enforcer }),
            Err(e) => Err(AuthorizationError::CasbinError(e)),
        }
    }

    /// Test if subject is allowed to perform given action on object
    ///
    /// ***subject*** The user object. _NOTE_: Must provide a role!
    /// ***object*** Either the HTTP path or the gRPC method
    /// ***action*** The intended action (CRUD)
    pub fn try_authorized(
        &self,
        subject: String,
        object: String,
        action: String,
    ) -> AuthorizationResult {
        let auth_data = (subject, object, action);

        dbg!(format!("Got tuple {:?} for authorization", auth_data));

        match self.enforcer.enforce(auth_data) {
            Ok(authorized) => {
                if authorized {
                    dbg!("Subject authorized");

                    Ok(AuthorizationState::Authorized)
                } else {
                    dbg!("Subject NOT authorized");

                    Ok(AuthorizationState::Denied)
                }
            }
            Err(e) => {
                dbg!(format!("error in authorization: {:?}", e));
                Err(AuthorizationError::CasbinError(e))
            }
        }
    }

    /// Helper for converting a String to &'static str
    fn string_to_static_str(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
}

/// Implement for all objects that shall be used for authorization
pub trait Authorizable {
    fn get_role(&self) -> String;
}

/// Implement for all objects that shall be able to test, if it's "owned" (i.e. has a FK constraint
/// in the DB) by given resource
#[axum::async_trait]
pub trait Owned<T, D> {
    async fn is_owned_by(resource: T, db: D) -> OwnershipState
        where D: 'static;
}

#[cfg(test)]
mod tests {}
