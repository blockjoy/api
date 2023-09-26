use std::collections::{HashMap, HashSet};

use chrono::Duration;
use derive_more::Deref;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;

use crate::database::Conn;
use crate::models::rbac::{RbacPerm, RbacUser};
use crate::models::{Host, Node};
use crate::timestamp::SecondsUtc;

use super::endpoint::Endpoints;
use super::rbac::{Access, Perm, Perms, Roles};
use super::resource::{HostId, NodeId, OrgId, Resource, ResourceEntry, Resources, UserId};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Claims does not have visibility of the target HostId.
    EnsureHost,
    /// Claims does not have visibility of the target NodeId.
    EnsureNode,
    /// Claims does not have visibility of the target OrgId.
    EnsureOrg,
    /// Claims does not have visibility of the target UserId.
    EnsureUser,
    /// Expiration time is before issue time.
    ExpiresBeforeIssued,
    /// Failed to check claims for host: {0},
    Host(#[from] crate::models::host::Error),
    /// Missing permission: {0}
    MissingPerm(Perm),
    /// Failed to check claims for node: {0},
    Node(#[from] crate::models::node::Error),
    /// Failed to check claims for org: {0},
    Org(#[from] crate::models::org::Error),
    /// Failed to check RBAC claims: {0},
    Rbac(#[from] crate::models::rbac::Error),
    /// Failed to check claims for user: {0},
    User(#[from] crate::models::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            EnsureHost | EnsureNode | EnsureOrg | EnsureUser => {
                Status::permission_denied("Access denied.")
            }
            ExpiresBeforeIssued => Status::internal("Internal error."),
            MissingPerm(perm) => Status::permission_denied(format!("Missing permission: {perm}")),
            Host(err) => err.into(),
            Node(err) => err.into(),
            Org(err) => err.into(),
            Rbac(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

/// A serializable representation of the auth token claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    #[serde(flatten)]
    pub resource_entry: ResourceEntry,
    #[serde(flatten)]
    pub expirable: Expirable,
    #[serde(flatten)]
    pub access: Access,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, String>>,
}

impl Claims {
    pub fn new(resource: Resource, expirable: Expirable, access: Access) -> Self {
        Claims {
            resource_entry: resource.into(),
            expirable,
            access,
            data: None,
        }
    }

    pub fn from_now<A, R>(expires: Duration, resource: R, access: A) -> Self
    where
        A: Into<Access>,
        R: Into<Resource>,
    {
        let expirable = Expirable::from_now(expires);
        Self::new(resource.into(), expirable, access.into())
    }

    pub fn with_data(mut self, data: HashMap<String, String>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn insert_data<K, V>(&mut self, key: K, value: V)
    where
        K: ToString,
        V: ToString,
    {
        match self.data {
            Some(ref mut data) => {
                data.insert(key.to_string(), value.to_string());
            }
            None => {
                self.data = Some(hashmap! { key.to_string() => value.to_string() });
            }
        }
    }

    pub fn resource(&self) -> Resource {
        self.resource_entry.into()
    }

    pub fn expires_at(&self) -> SecondsUtc {
        self.expirable.expires_at
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.data
            .as_ref()
            .and_then(|data| data.get(key))
            .map(|val| val.as_str())
    }

    /// Ensure that `Claims` can access the target `Resources`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_resources(
        &self,
        resources: Resources,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        match resources {
            Resources::One(resource) => self.ensure_resource(resource, conn).await,
            Resources::Many(resources) => {
                let mut granted = Granted::default();
                for resource in resources {
                    if let Some(perms) = self.ensure_resource(resource, conn).await? {
                        granted.join(&perms);
                    }
                }
                Ok(Some(granted))
            }
        }
    }

    /// Ensure that `Claims` can access the target `Resource`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_resource(
        &self,
        resource: Resource,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        match resource {
            Resource::User(id) => self.ensure_user(id, conn).await,
            Resource::Org(id) => self.ensure_org(id, conn).await,
            Resource::Host(id) => self.ensure_host(id, conn).await,
            Resource::Node(id) => self.ensure_node(id, conn).await,
        }
    }

    /// Ensure that `Claims` can access the target `UserId`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_user(
        &self,
        user_id: UserId,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        match self.resource() {
            Resource::User(id) if id == user_id => {
                Ok(RbacUser::admin_perms(id, conn).await?.map(Granted))
            }
            _ => Err(Error::EnsureUser),
        }
    }

    /// Ensure that `Claims` can access the target `OrgId`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_org(
        &self,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        match self.resource() {
            Resource::User(id) => Ok(Some(Granted(RbacPerm::for_org(id, org_id, conn).await?))),
            Resource::Org(id) if id == org_id => Ok(None),
            _ => Err(Error::EnsureOrg),
        }
    }

    /// Ensure that `Claims` can access the target `HostId`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_host(
        &self,
        host_id: HostId,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        let host = Host::find_by_id(host_id, conn).await?;

        match self.resource() {
            Resource::User(id) => Ok(Some(Granted(
                RbacPerm::for_org(id, host.org_id, conn).await?,
            ))),
            Resource::Org(id) if id == host.org_id => Ok(None),
            Resource::Host(id) if id == host_id => Ok(None),
            _ => Err(Error::EnsureHost),
        }
    }

    /// Ensure that `Claims` can access the target `NodeId`.
    ///
    /// Returns any additional permissions granted during authorization.
    pub async fn ensure_node(
        &self,
        node_id: NodeId,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Granted>, Error> {
        let node = Node::find_by_id(node_id, conn).await?;

        match self.resource() {
            Resource::User(id) => Ok(Some(Granted(
                RbacPerm::for_org(id, node.org_id, conn).await?,
            ))),
            Resource::Org(id) if id == node.org_id => Ok(None),
            Resource::Host(id) if id == node.host_id => Ok(None),
            Resource::Node(id) if id == node_id => Ok(None),
            _ => Err(Error::EnsureNode),
        }
    }
}

/// A set of permissions granted by authorization checks.
#[derive(Debug, Default, Deref)]
pub struct Granted(HashSet<Perm>);

impl Granted {
    /// Returns all permissions granted based on `Access` claims.
    ///
    /// Optionally accepts an input set of permissions already granted.
    pub async fn from_access(
        access: &Access,
        initial: Option<Granted>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut granted = initial.unwrap_or_default();

        match access {
            Access::Perms(Perms::One(perm)) => granted.push(*perm),
            Access::Perms(Perms::Many(perms)) => granted.join(perms),

            Access::Roles(Roles::One(role)) => {
                granted.join(&RbacPerm::for_role(*role, conn).await?)
            }
            Access::Roles(Roles::Many(roles)) => {
                granted.join(&RbacPerm::for_roles(roles, conn).await?)
            }

            Access::Endpoints(Endpoints::Single(endpoint)) => match (*endpoint).into() {
                Perms::One(perm) => granted.push(perm),
                Perms::Many(perms) => granted.join(&perms),
            },
            Access::Endpoints(Endpoints::Multiple(endpoints)) => {
                for endpoint in endpoints {
                    match (*endpoint).into() {
                        Perms::One(perm) => granted.push(perm),
                        Perms::Many(perms) => granted.join(&perms),
                    }
                }
            }
        }

        Ok(granted)
    }

    /// Returns permissions granted based on blockjoy admin role membership.
    pub async fn from_admin(user_id: UserId, conn: &mut Conn<'_>) -> Result<Option<Self>, Error> {
        Ok(RbacUser::admin_perms(user_id, conn).await?.map(Self))
    }

    pub async fn for_org(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        RbacPerm::for_org(user_id, org_id, conn)
            .await
            .map(Self)
            .map_err(Into::into)
    }

    fn push<P>(&mut self, perm: P)
    where
        P: Into<Perm>,
    {
        self.0.insert(perm.into());
    }

    fn join(&mut self, perms: &HashSet<Perm>) {
        self.0.extend(perms)
    }

    pub fn has_perm<P>(&self, perm: P) -> bool
    where
        P: Into<Perm>,
    {
        self.contains(&perm.into())
    }

    pub fn ensure_perm<P>(&self, perm: P) -> Result<(), Error>
    where
        P: Into<Perm>,
    {
        let perm = perm.into();
        self.has_perm(perm)
            .then_some(())
            .ok_or(Error::MissingPerm(perm))
    }

    pub fn ensure_perms(&self, perms: HashSet<Perm>) -> Result<(), Error> {
        for perm in perms {
            self.has_perm(perm)
                .then_some(())
                .ok_or(Error::MissingPerm(perm))?
        }

        Ok(())
    }
}

/// `Expirable` ensures that `issued_at` is not after `expires_at`.
///
/// It also serializes and deserializes with second precision since JWTs don't
/// support nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expirable {
    #[serde(rename = "iat")]
    pub issued_at: SecondsUtc,
    #[serde(rename = "exp")]
    pub expires_at: SecondsUtc,
}

impl Expirable {
    pub fn new(issued_at: SecondsUtc, expires_at: SecondsUtc) -> Result<Self, Error> {
        if expires_at < issued_at {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(Self {
            issued_at,
            expires_at,
        })
    }

    pub fn from_now(expires: Duration) -> Self {
        let issued_at = SecondsUtc::now();
        let expires_at = issued_at + expires;

        Self {
            issued_at,
            expires_at,
        }
    }

    pub fn duration(&self) -> Duration {
        self.expires_at - self.issued_at
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    #[allow(unused_imports)]
    use crate::auth::endpoint::Endpoint;

    use super::*;

    pub fn claims_none(user_id: UserId) -> Claims {
        let expires = Duration::minutes(15);

        Claims {
            resource_entry: ResourceEntry::new_user(user_id),
            expirable: Expirable::from_now(expires),
            access: Access::Endpoints(Endpoints::Multiple(vec![])),
            data: None,
        }
    }

    #[test]
    fn can_parse_one_endpoint() {
        let json = r#"
            {
                "resource_type": "User",
                "resource_id": "5a606a36-d530-4c1b-95a9-342ad4d66686",
                "iat": 1690300850,
                "exp": 1690301750,
                "endpoints": "NodeCreate"
            }"#;

        let claims: Claims = serde_json::from_str(json).unwrap();
        assert_eq!(
            claims.access,
            Access::Endpoints(Endpoints::Single(Endpoint::NodeCreate))
        );
        assert!(claims.data.is_none());
    }

    #[test]
    fn can_parse_multiple_endpoints() {
        let json = r#"
            {
                "resource_type": "User",
                "resource_id": "5a606a36-d530-4c1b-95a9-342ad4d66686",
                "iat": 1690300850,
                "exp": 1690301750,
                "endpoints": ["HostStart", "HostStop"]
            }"#;

        let claims: Claims = serde_json::from_str(json).unwrap();
        assert_eq!(
            claims.access,
            Access::Endpoints(Endpoints::Multiple(vec![
                Endpoint::HostStart,
                Endpoint::HostStop
            ]))
        );
    }
}
