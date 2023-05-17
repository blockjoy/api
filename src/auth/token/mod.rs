use std::collections::HashMap;

mod api_key;
mod jwt;
mod refresh;

pub use api_key::ApiKey;
pub use jwt::Jwt;
pub use refresh::Refresh;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub resource_type: ResourceType,
    pub resource_id: uuid::Uuid,
    #[serde(with = "timestamp")]
    pub iat: chrono::DateTime<chrono::Utc>,
    #[serde(with = "timestamp")]
    pub exp: chrono::DateTime<chrono::Utc>,
    pub endpoints: Endpoints,
    pub data: HashMap<String, String>,
}

impl Claims {
    pub fn resource(&self) -> Resource {
        match self.resource_type {
            ResourceType::User => Resource::User(self.resource_id),
            ResourceType::Org => Resource::Org(self.resource_id),
            ResourceType::Host => Resource::Host(self.resource_id),
            ResourceType::Node => Resource::Node(self.resource_id),
        }
    }

    pub fn new_user(
        user_id: uuid::Uuid,
        iat: chrono::DateTime<chrono::Utc>,
        exp: chrono::Duration,
        endpoints: impl IntoIterator<Item = Endpoint>,
    ) -> Self {
        Self {
            resource_type: ResourceType::User,
            resource_id: user_id,
            iat,
            exp: iat + exp,
            endpoints: endpoints.into_iter().collect(),
            data: HashMap::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Resource {
    User(uuid::Uuid),
    Org(uuid::Uuid),
    Host(uuid::Uuid),
    Node(uuid::Uuid),
}

impl Resource {
    /// Applies the function `f` if the current `Resource` is of the variant `User`, then returns an
    /// `Option` containing the result of the application, otherwise returns `None`.
    pub fn map_user<T, F: FnOnce(uuid::Uuid) -> T>(&self, f: F) -> Option<T> {
        match *self {
            Self::User(id) => Some(f(id)),
            _ => None,
        }
    }
}

/// The types of resources that can grant authorization. For example, a user has access to all nodes
/// it has created, but a host also has access to all nodes that run on it. They are hierarchically
/// sorted here, which is to say that a user has multiple orgs, an org has multiple hosts and a host
/// has multiple nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResourceType {
    User,
    Org,
    Host,
    Node,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ClaimsRole {
    Admin,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Endpoints {
    #[serde(rename = "*")]
    Wildcard,
    Single(Endpoint),
    Multiple(Vec<Endpoint>),
}

impl Endpoints {
    pub(super) fn includes(&self, endpoint: Endpoint) -> bool {
        match self {
            Self::Wildcard => true,
            Self::Single(this) => this.matches(endpoint),
            Self::Multiple(these) => these.iter().any(|this| this.matches(endpoint)),
        }
    }
}

impl FromIterator<Endpoint> for Endpoints {
    fn from_iter<T: IntoIterator<Item = Endpoint>>(iter: T) -> Self {
        let mut iter = iter.into_iter();
        let Some(first) = iter.next() else { return Self::Multiple(vec![]) };
        let second = iter.next();
        match second {
            Some(second) => {
                let mut items = vec![first, second];
                items.extend(iter);
                Self::Multiple(items)
            }
            None => Self::Single(first),
        }
    }
}

mod timestamp {
    use chrono::TimeZone;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        date: &chrono::DateTime<chrono::Utc>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let ts = date.timestamp();
        serializer.serialize_i64(ts)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<chrono::DateTime<chrono::Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i = i64::deserialize(deserializer)?;
        match chrono::Utc.timestamp_opt(i, 0) {
            chrono::LocalResult::None => Err(serde::de::Error::custom("Invalid timestamp")),
            chrono::LocalResult::Single(t) => Ok(t),
            chrono::LocalResult::Ambiguous(t, _) => Ok(t),
        }
    }
}

/// This enum is used to uniquely determine an endpoint or service in our authentication process.
/// For example, the endpoint `blockjoy.v1.CommandService/Create` is determined by the variant
/// `CommandCreate`. This is then in turn used by the authentication flow: is a user allowed to
/// access a specific endpoint. Even though it is chiefly used for this, all endpoints are
/// represented here, even the ones that do not require any authorization such as `BlockchainGet`.
/// For each service we reserve 100 numbers of space. Reserving this space makes it simple to do
/// comparison checks, but if we run out of it (by creating a service with over 100 endpoints??)
/// then we can work around this.
///
/// The variants for each service as seperated by a blank line. Note that the first variant of each
/// service acts as a wildcard for the entire service. This allows us to grant broad access to a
/// particular service, without bloating the token.
#[repr(u64)] // Should be enough :)
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Endpoint {
    AuthAll = 0,
    AuthConfirm = 1,
    AuthRefresh = 2,
    AuthResetPassword = 3,
    AuthUpdatePassword = 4,

    BlockchainAll = 100,
    BlockchainList = 101,
    BlockchainGet = 102,

    CommandAll = 200,
    CommandCreate = 201,
    CommandGet = 202,
    CommandUpdate = 203,
    CommandPending = 204,

    DiscoveryAll = 300,
    DiscoveryServices = 301,

    HostAll = 400,
    HostCreate = 401,
    HostGet = 402,
    HostList = 403,
    HostUpdate = 404,
    HostDelete = 405,
    HostProvision = 406,

    HostProvisionAll = 500,
    HostProvisionGet = 501,
    HostProvisionCreate = 502,

    InvitationAll = 600,
    InvitationCreate = 601,
    InvitationList = 602,
    InvitationAccept = 603,
    InvitationDecline = 605,
    InvitationRevoke = 607,

    KeyFileAll = 700,
    KeyFileCreate = 701,
    KeyFileList = 702,

    MetricsAll = 800,
    MetricsNode = 801,
    MetricsHost = 802,

    NodeAll = 900,
    NodeCreate = 901,
    NodeGet = 902,
    NodeList = 903,
    NodeUpdate = 904,
    NodeDelete = 905,

    OrgAll = 1000,
    OrgCreate = 1001,
    OrgGet = 1002,
    OrgList = 1003,
    OrgUpdate = 1004,
    OrgDelete = 1005,
    OrgRemoveMember = 1006,

    UserAll = 1100,
    UserGet = 1101,
    UserUpdate = 1102,
    UserDelete = 1103,

    BabelAll = 1200,
    BabelNotifiy = 1201,
}

const SPACE_PER_SERVICE: u64 = 100;

impl Endpoint {
    /// This function checks whether two endpoints match. We define `matching` as that they are the
    /// same exact endpoint, or that one of them is `<ServiceName>All`, and the other service is of
    /// the form `<ServiceName><EndpointName>`, with `<ServiceName>` being the same in both cases.
    /// This function is commutative, which means that you can swap self and other, and get the
    /// exact same results.
    fn matches(self, other: Self) -> bool {
        let exact_match = self == other;
        let is_all = self as u64 % SPACE_PER_SERVICE == 0 || other as u64 % SPACE_PER_SERVICE == 0;
        let same_service = self as u64 / SPACE_PER_SERVICE == other as u64 / SPACE_PER_SERVICE;

        exact_match || (is_all && same_service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_matches() {
        use Endpoint::*;
        let selfs = [HostAll, CommandAll, KeyFileCreate, OrgDelete];
        let others = [HostCreate, CommandAll, CommandUpdate, NodeDelete, OrgAll];
        let expected = [
            [true, false, false, false, false],
            [false, true, true, false, false],
            [false, false, false, false, false],
            [false, false, false, false, true],
        ];
        for (this, expected) in selfs.iter().zip(expected) {
            for (other, expected) in others.iter().copied().zip(expected) {
                assert_eq!(
                    this.matches(other),
                    expected,
                    "Expected {this:?}.matches({other:?}) to be {expected}"
                );
            }
        }
    }
}
