use std::collections::{HashMap, HashSet};

use chrono::Duration;
use derive_more::Deref;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::database::Conn;
use crate::grpc::Status;
use crate::model::rbac::{RbacPerm, RbacUser};
use crate::model::{Host, Node};
use crate::util::SecondsUtc;

use super::rbac::{Access, Perm, Perms, Roles};
use super::resource::{HostId, NodeId, OrgId, Resource, ResourceEntry, Resources, UserId};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Claims `{0:?}` does not have visibility of the target HostId ({1}).
    EnsureHost(Resource, HostId),
    /// Claims `{0:?}` does not have visibility of the target NodeId ({1}).
    EnsureNode(Resource, NodeId),
    /// Claims `{0:?}` does not have visibility of the target OrgId ({1}).
    EnsureOrg(Resource, OrgId),
    /// Claims `{0:?}` does not have visibility of the target UserId ({1}).
    EnsureUser(Resource, UserId),
    /// Failed to check claims for host: {0},
    Host(#[from] crate::model::host::Error),
    /// Permission `{0}` not held by {1}
    MissingPerm(Perm, Resource),
    /// Failed to check claims for node: {0},
    Node(#[from] crate::model::node::Error),
    /// Failed to check claims for org: {0},
    Org(#[from] crate::model::org::Error),
    /// Failed to check RBAC claims: {0},
    Rbac(#[from] crate::model::rbac::Error),
    /// Failed to check claims for user: {0},
    User(#[from] crate::model::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            EnsureHost(..) | EnsureNode(..) | EnsureOrg(..) | EnsureUser(..) => {
                Status::forbidden("Access denied.")
            }
            MissingPerm(perm, _) => Status::forbidden(format!("Missing permission: {perm}")),
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

    #[must_use]
    pub fn with_data(mut self, data: HashMap<String, String>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn insert_data<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        match self.data {
            Some(ref mut data) => {
                data.insert(key.into(), value.into());
            }
            None => {
                self.data = Some(hashmap! { key.into() => value.into() });
            }
        }
    }

    pub fn resource(&self) -> Resource {
        self.resource_entry.into()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.data
            .as_ref()
            .and_then(|data| data.get(key))
            .map(String::as_str)
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
            Resource::User(id) => self.ensure_user(id).map(|()| None),
            Resource::Org(id) => self.ensure_org(id, conn).await,
            Resource::Host(id) => self.ensure_host(id, conn).await,
            Resource::Node(id) => self.ensure_node(id, conn).await,
        }
    }

    /// Ensure that `Claims` can access the target `UserId`.
    pub fn ensure_user(&self, user_id: UserId) -> Result<(), Error> {
        match self.resource() {
            Resource::User(id) if id == user_id => Ok(()),
            resource => Err(Error::EnsureUser(resource, user_id)),
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
            Resource::User(id) => Ok(Some(Granted(
                RbacPerm::for_org(id, org_id, true, conn).await?,
            ))),
            Resource::Org(id) if id == org_id => Ok(None),
            resource => Err(Error::EnsureOrg(resource, org_id)),
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
        let host = Host::by_id(host_id, conn).await?;

        match self.resource() {
            Resource::User(id) => Ok(Some(Granted(
                RbacPerm::for_org(id, host.org_id, true, conn).await?,
            ))),
            Resource::Org(id) if id == host.org_id => Ok(None),
            Resource::Host(id) if id == host_id => Ok(None),
            resource => Err(Error::EnsureHost(resource, host_id)),
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
        let node = Node::by_id(node_id, conn).await?;

        match self.resource() {
            Resource::User(id) => Ok(Some(Granted(
                RbacPerm::for_org(id, node.org_id, true, conn).await?,
            ))),
            Resource::Org(id) if id == node.org_id => Ok(None),
            Resource::Host(id) if id == node.host_id => Ok(None),
            Resource::Node(id) if id == node_id => Ok(None),
            resource => Err(Error::EnsureNode(resource, node_id)),
        }
    }
}

/// A set of permissions granted by authorization checks.
#[derive(Debug, Default, Deref)]
pub struct Granted(HashSet<Perm>);

impl Granted {
    /// All permissions granted for roles that don't depend on the org.
    ///
    /// Optionally accepts an input set of permissions already granted.
    pub async fn all_orgs(
        user_id: UserId,
        granted: Option<Granted>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut granted = granted.unwrap_or_default();
        let perms = RbacUser::perms_for_non_org_roles(user_id, conn).await?;

        granted.join(&perms);
        Ok(granted)
    }

    /// All permissions granted based on `Access` claims.
    ///
    /// Optionally accepts an input set of permissions already granted.
    pub async fn from_access(
        access: &Access,
        granted: Option<Granted>,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut granted = granted.unwrap_or_default();

        match access {
            Access::Perms(Perms::One(perm)) => granted.push(*perm),
            Access::Perms(Perms::Many(perms)) => granted.join(perms),

            Access::Roles(Roles::One(role)) => {
                granted.join(&RbacPerm::for_role(*role, conn).await?);
            }
            Access::Roles(Roles::Many(roles)) => {
                granted.join(&RbacPerm::for_roles(roles, conn).await?);
            }
        }

        Ok(granted)
    }

    pub async fn for_org(
        user_id: UserId,
        org_id: OrgId,
        ensure_member: bool,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        RbacPerm::for_org(user_id, org_id, ensure_member, conn)
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
        self.0.extend(perms);
    }

    pub fn has_perm<P>(&self, perm: P) -> bool
    where
        P: Into<Perm>,
    {
        self.contains(&perm.into())
    }

    pub fn has_any_perm<I, P>(&self, perms: I) -> bool
    where
        I: IntoIterator<Item = P>,
        P: Into<Perm>,
    {
        perms.into_iter().any(|perm| self.contains(&perm.into()))
    }

    pub fn ensure_perm<P>(&self, perm: P, claims: &Claims) -> Result<(), Error>
    where
        P: Into<Perm>,
    {
        let perm = perm.into();
        self.has_perm(perm)
            .then_some(())
            .ok_or_else(|| Error::MissingPerm(perm, claims.resource()))
    }

    pub fn ensure_perms(&self, perms: HashSet<Perm>, claims: &Claims) -> Result<(), Error> {
        perms
            .into_iter()
            .try_for_each(|p| self.ensure_perm(p, claims))
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

#[cfg(test)]
mod tests {
    use crate::auth::rbac::{HostPerm, NodePerm};

    use super::*;

    #[test]
    fn can_parse_one_perm() {
        let json = r#"{
            "resource_type": "User",
            "resource_id": "5a606a36-d530-4c1b-95a9-342ad4d66686",
            "iat": 1690300850,
            "exp": 1690301750,
            "perms": "node-create"
        }"#;

        let claims: Claims = serde_json::from_str(json).unwrap();
        assert_eq!(
            claims.access,
            Access::Perms(Perms::One(NodePerm::Create.into()))
        );
        assert!(claims.data.is_none());
    }

    #[test]
    fn can_parse_many_perms() {
        let json = r#"{
            "resource_type": "User",
            "resource_id": "5a606a36-d530-4c1b-95a9-342ad4d66686",
            "iat": 1690300850,
            "exp": 1690301750,
            "perms": ["host-start", "host-stop"]
        }"#;

        let claims: Claims = serde_json::from_str(json).unwrap();
        assert_eq!(
            claims.access,
            Access::Perms(Perms::Many(hashset! {
                HostPerm::Start.into(),
                HostPerm::Stop.into()
            }))
        );
    }
}
