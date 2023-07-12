use serde::{Deserialize, Serialize};

const SPACE_PER_SERVICE: u64 = 100;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Endpoint {
    AuthAll = 0,
    AuthConfirm = 1,
    AuthRefresh = 2,
    AuthResetPassword = 3,
    AuthUpdatePassword = 4,
    AuthUpdateUiPassword = 5,

    BlockchainAll = 100,
    BlockchainList = 101,
    BlockchainGet = 102,

    CommandAll = 200,
    CommandCreate = 201,
    CommandGet = 202,
    CommandUpdate = 203,
    CommandPending = 204,
    CommandAck = 205,

    DiscoveryAll = 300,
    DiscoveryServices = 301,

    HostAll = 400,
    HostCreate = 401,
    HostGet = 402,
    HostList = 403,
    HostUpdate = 404,
    HostDelete = 405,
    HostProvision = 406,
    HostStart = 407,
    HostStop = 408,
    HostRestart = 409,

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
    NodeUpdateConfig = 904,
    NodeDelete = 905,
    NodeUpdateStatus = 906,
    NodeStart = 907,
    NodeStop = 908,
    NodeRestart = 909,

    OrgAll = 1000,
    OrgCreate = 1001,
    OrgGet = 1002,
    OrgList = 1003,
    OrgUpdate = 1004,
    OrgDelete = 1005,
    OrgRemoveMember = 1006,
    OrgGetProvisionToken = 1007,
    OrgResetProvisionToken = 1008,

    UserAll = 1100,
    UserCreate = 1101,
    UserGet = 1102,
    UserUpdate = 1103,
    UserDelete = 1104,
    UserGetBilling = 1105,
    UserUpdateBilling = 1106,
    UserDeleteBilling = 1107,

    BabelAll = 1200,
    BabelNotify = 1201,

    CookbookAll = 1300,
    CookbookRetrievePlugin = 1301,
    CookbookRetrieveImage = 1302,
    CookbookRetrieveKernel = 1303,
    CookbookRequirements = 1304,
    CookbookNetConfigurations = 1305,
    CookbookListBabelVersions = 1306,

    BundleAll = 1400,
    BundleRetrieve = 1401,
    BundleListBundleVersions = 1402,
    BundleDelete = 1403,

    ManifestAll = 1501,
    ManifestRetrieveDownload = 1502,

    ApiKeyAll = 1600,
    ApiKeyCreate = 1601,
    ApiKeyList = 1602,
    ApiKeyUpdate = 1603,
    ApiKeyRegenerate = 1604,
    ApiKeyDelete = 1605,

    SubscriptionAll = 1700,
    SubscriptionCreate = 1701,
    SubscriptionGet = 1702,
    SubscriptionList = 1703,
    SubscriptionDelete = 1704,
}

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

impl From<&Endpoint> for Endpoint {
    fn from(endpoint: &Endpoint) -> Self {
        *endpoint
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Endpoints {
    #[serde(rename = "*")]
    Wildcard,
    Single(Endpoint),
    Multiple(Vec<Endpoint>),
}

impl Endpoints {
    pub fn includes(&self, endpoint: Endpoint) -> bool {
        match self {
            Self::Wildcard => true,
            Self::Single(this) => this.matches(endpoint),
            Self::Multiple(these) => these.iter().any(|this| this.matches(endpoint)),
        }
    }
}

impl FromIterator<Endpoint> for Endpoints {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Endpoint>,
    {
        let mut iter = iter.into_iter();

        match (iter.next(), iter.next()) {
            (Some(first), None) => Endpoints::Single(first),
            (Some(first), Some(second)) => {
                let mut items = vec![first, second];
                items.extend(iter);
                Endpoints::Multiple(items)
            }
            _ => Endpoints::Multiple(vec![]),
        }
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
