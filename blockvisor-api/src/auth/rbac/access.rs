use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{Perm, Role};

/// Access determines what permissions are available to token `Claims`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Roles(Roles),
    Perms(Perms),
}

impl From<Roles> for Access {
    fn from(roles: Roles) -> Self {
        Access::Roles(roles)
    }
}

impl From<Perms> for Access {
    fn from(perms: Perms) -> Self {
        Access::Perms(perms)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Roles {
    One(Role),
    Many(HashSet<Role>),
}

impl<I, R> From<I> for Roles
where
    I: IntoIterator<Item = R>,
    R: Into<Role>,
{
    fn from(items: I) -> Self {
        Roles::Many(items.into_iter().map(Into::into).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Perms {
    One(Perm),
    All(HashSet<Perm>),
    Any(HashSet<Perm>),
}

impl From<Perm> for Perms {
    fn from(perm: Perm) -> Self {
        Perms::One(perm)
    }
}

impl<I, P> From<I> for Perms
where
    I: IntoIterator<Item = P>,
    P: Into<Perm>,
{
    fn from(items: I) -> Self {
        Perms::All(items.into_iter().map(Into::into).collect())
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use crate::auth::claims::{Claims, Granted};
    use crate::auth::rbac::{ProtocolAdminPerm, ProtocolPerm};
    use crate::auth::resource::Resource;
    use crate::auth::AuthZ;

    #[cfg(test)]
    use crate::auth::rbac::{
        ApiKeyPerm, ApiKeyRole, BlockjoyRole, GrpcRole, HostPerm, HostProvisionPerm,
    };

    use super::*;

    #[allow(clippy::implicit_hasher)]
    pub fn authz<R>(perms: HashSet<Perm>, resource: R) -> AuthZ
    where
        R: Into<Resource> + Send,
    {
        let expires = chrono::Duration::minutes(15);
        let access = Access::Perms(Perms::All(perms.clone()));

        AuthZ {
            claims: Claims::from_now(expires, resource.into(), access),
            granted: Granted::test_with(perms),
        }
    }

    pub fn view_authz<R>(resource: R) -> AuthZ
    where
        R: Into<Resource> + Send,
    {
        let perms = hashset! {
            ProtocolAdminPerm::ViewAllStats.into(),
            ProtocolAdminPerm::ViewPrivate.into(),
            ProtocolPerm::ViewDevelopment.into(),
            ProtocolPerm::ViewPublic.into(),
        };
        authz(perms, resource)
    }

    #[derive(Serialize, Deserialize)]
    struct TestRoles {
        roles: Roles,
    }

    #[derive(Serialize, Deserialize)]
    struct TestPerms {
        perms: Perms,
    }

    #[test]
    fn serde_one_role() {
        let json = r#"{"roles":"blockjoy-admin"}"#;
        let roles: TestRoles = serde_json::from_str(json).unwrap();

        let expected = Roles::One(BlockjoyRole::Admin.into());
        assert_eq!(roles.roles, expected);

        let serialized = serde_json::to_string(&roles).unwrap();
        assert_eq!(serialized, json);
    }

    #[test]
    fn serde_many_roles() {
        let json = r#"{"roles":["api-key-user","grpc-new-host"]}"#;
        let roles: TestRoles = serde_json::from_str(json).unwrap();

        let expected = Roles::Many(hashset! { ApiKeyRole::User.into(), GrpcRole::NewHost.into() });
        assert_eq!(roles.roles, expected);
    }

    #[test]
    fn serde_one_perm() {
        let json = r#"{"perms":"api-key-create"}"#;
        let perms: TestPerms = serde_json::from_str(json).unwrap();

        let expected = Perms::One(ApiKeyPerm::Create.into());
        assert_eq!(perms.perms, expected);

        let serialized = serde_json::to_string(&perms).unwrap();
        assert_eq!(serialized, json);
    }

    #[test]
    fn serde_many_perms() {
        let json = r#"{"perms":["host-get","host-provision-get"]}"#;
        let perms: TestPerms = serde_json::from_str(json).unwrap();

        let expected = Perms::All(hashset! { HostPerm::Get.into(), HostProvisionPerm::Get.into() });
        assert_eq!(perms.perms, expected);
    }
}
