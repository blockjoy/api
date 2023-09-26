//! TODO: Delete this module when there are no outstanding JWT Endpoint tokens.

use serde::{Deserialize, Serialize};

use super::rbac::*;

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
    HostStart = 406,
    HostStop = 407,
    HostRestart = 408,
    HostRegions = 409,

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
    UserFilter = 1108,

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

    ManifestAll = 1500,
    ManifestRetrieveDownload = 1501,

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

impl From<Endpoint> for Perms {
    fn from(endpoint: Endpoint) -> Self {
        match endpoint {
            Endpoint::AuthAll => Perms::Many(hashset! {
                AuthPerm::Confirm.into(),
                AuthPerm::Refresh.into(),
                AuthPerm::ResetPassword.into(),
                AuthPerm::UpdatePassword.into(),
                AuthPerm::UpdateUiPassword.into()
            }),
            Endpoint::AuthConfirm => AuthPerm::Confirm.into(),
            Endpoint::AuthRefresh => AuthPerm::Refresh.into(),
            Endpoint::AuthResetPassword => AuthPerm::ResetPassword.into(),
            Endpoint::AuthUpdatePassword => AuthPerm::UpdatePassword.into(),
            Endpoint::AuthUpdateUiPassword => AuthPerm::UpdateUiPassword.into(),

            Endpoint::BlockchainAll => Perms::Many(hashset! {
                BlockchainPerm::List.into(),
                BlockchainPerm::Get.into()
            }),
            Endpoint::BlockchainList => BlockchainPerm::List.into(),
            Endpoint::BlockchainGet => BlockchainPerm::Get.into(),

            Endpoint::CommandAll => Perms::Many(hashset! {
                CommandPerm::Create.into(),
                CommandPerm::Get.into(),
                CommandPerm::Update.into(),
                CommandPerm::Pending.into(),
                CommandPerm::Ack.into()
            }),
            Endpoint::CommandCreate => CommandPerm::Create.into(),
            Endpoint::CommandGet => CommandPerm::Get.into(),
            Endpoint::CommandUpdate => CommandPerm::Update.into(),
            Endpoint::CommandPending => CommandPerm::Pending.into(),
            Endpoint::CommandAck => CommandPerm::Ack.into(),

            Endpoint::DiscoveryAll => Perms::Many(hashset! {
                DiscoveryPerm::Services.into()
            }),
            Endpoint::DiscoveryServices => DiscoveryPerm::Services.into(),

            Endpoint::HostAll => Perms::Many(hashset! {
                HostPerm::Create.into(),
                HostPerm::Get.into(),
                HostPerm::List.into(),
                HostPerm::Update.into(),
                HostPerm::Delete.into(),
                HostPerm::Start.into(),
                HostPerm::Stop.into(),
                HostPerm::Restart.into(),
                HostPerm::Regions.into()
            }),
            Endpoint::HostCreate => HostPerm::Create.into(),
            Endpoint::HostGet => HostPerm::Get.into(),
            Endpoint::HostList => HostPerm::List.into(),
            Endpoint::HostUpdate => HostPerm::Update.into(),
            Endpoint::HostDelete => HostPerm::Delete.into(),
            Endpoint::HostStart => HostPerm::Start.into(),
            Endpoint::HostStop => HostPerm::Stop.into(),
            Endpoint::HostRestart => HostPerm::Restart.into(),
            Endpoint::HostRegions => HostPerm::Regions.into(),

            Endpoint::HostProvisionAll => Perms::Many(hashset! {
                HostProvisionPerm::Get.into(),
                HostProvisionPerm::Create.into()
            }),
            Endpoint::HostProvisionGet => HostProvisionPerm::Get.into(),
            Endpoint::HostProvisionCreate => HostProvisionPerm::Create.into(),

            Endpoint::InvitationAll => Perms::Many(hashset! {
                InvitationPerm::Create.into(),
                InvitationPerm::List.into(),
                InvitationPerm::Accept.into(),
                InvitationPerm::Decline.into(),
                InvitationPerm::Revoke.into()
            }),
            Endpoint::InvitationCreate => InvitationPerm::Create.into(),
            Endpoint::InvitationList => InvitationPerm::List.into(),
            Endpoint::InvitationAccept => InvitationPerm::Accept.into(),
            Endpoint::InvitationDecline => InvitationPerm::Decline.into(),
            Endpoint::InvitationRevoke => InvitationPerm::Revoke.into(),

            Endpoint::KeyFileAll => Perms::Many(hashset! {
                KeyFilePerm::Create.into(),
                KeyFilePerm::List.into()
            }),
            Endpoint::KeyFileCreate => KeyFilePerm::Create.into(),
            Endpoint::KeyFileList => KeyFilePerm::List.into(),

            Endpoint::MetricsAll => Perms::Many(hashset! {
                MetricsPerm::Node.into(),
                MetricsPerm::Host.into()
            }),
            Endpoint::MetricsNode => MetricsPerm::Node.into(),
            Endpoint::MetricsHost => MetricsPerm::Host.into(),

            Endpoint::NodeAll => Perms::Many(hashset! {
                NodePerm::Create.into(),
                NodePerm::Get.into(),
                NodePerm::List.into(),
                NodePerm::UpdateConfig.into(),
                NodePerm::Delete.into(),
                NodePerm::UpdateStatus.into(),
                NodePerm::Start.into(),
                NodePerm::Stop.into(),
                NodePerm::Restart.into()
            }),
            Endpoint::NodeCreate => NodePerm::Create.into(),
            Endpoint::NodeGet => NodePerm::Get.into(),
            Endpoint::NodeList => NodePerm::List.into(),
            Endpoint::NodeUpdateConfig => NodePerm::UpdateConfig.into(),
            Endpoint::NodeDelete => NodePerm::Delete.into(),
            Endpoint::NodeUpdateStatus => NodePerm::UpdateStatus.into(),
            Endpoint::NodeStart => NodePerm::Start.into(),
            Endpoint::NodeStop => NodePerm::Stop.into(),
            Endpoint::NodeRestart => NodePerm::Restart.into(),

            Endpoint::OrgAll => Perms::Many(hashset! {
                OrgPerm::Create.into(),
                OrgPerm::Get.into(),
                OrgPerm::List.into(),
                OrgPerm::Update.into(),
                OrgPerm::Delete.into(),
                OrgPerm::RemoveMember.into(),
                OrgProvisionPerm::GetToken.into(),
                OrgProvisionPerm::ResetToken.into()
            }),
            Endpoint::OrgCreate => OrgPerm::Create.into(),
            Endpoint::OrgGet => OrgPerm::Get.into(),
            Endpoint::OrgList => OrgPerm::List.into(),
            Endpoint::OrgUpdate => OrgPerm::Update.into(),
            Endpoint::OrgDelete => OrgPerm::Delete.into(),
            Endpoint::OrgRemoveMember => OrgPerm::RemoveMember.into(),
            Endpoint::OrgGetProvisionToken => OrgProvisionPerm::GetToken.into(),
            Endpoint::OrgResetProvisionToken => OrgProvisionPerm::ResetToken.into(),

            Endpoint::UserAll => Perms::Many(hashset! {
                UserPerm::Create.into(),
                UserPerm::Get.into(),
                UserPerm::Update.into(),
                UserPerm::Delete.into(),
                UserBillingPerm::Get.into(),
                UserBillingPerm::Update.into(),
                UserBillingPerm::Delete.into()
            }),
            Endpoint::UserCreate => UserPerm::Create.into(),
            Endpoint::UserGet => UserPerm::Get.into(),
            Endpoint::UserUpdate => UserPerm::Update.into(),
            Endpoint::UserDelete => UserPerm::Delete.into(),
            Endpoint::UserGetBilling => UserBillingPerm::Get.into(),
            Endpoint::UserUpdateBilling => UserBillingPerm::Update.into(),
            Endpoint::UserDeleteBilling => UserBillingPerm::Delete.into(),
            Endpoint::UserFilter => UserPerm::Filter.into(),

            Endpoint::BabelAll => Perms::Many(hashset! {
                BabelPerm::Notify.into()
            }),
            Endpoint::BabelNotify => BabelPerm::Notify.into(),

            Endpoint::CookbookAll => Perms::Many(hashset! {
                CookbookPerm::RetrievePlugin.into(),
                CookbookPerm::RetrieveImage.into(),
                CookbookPerm::RetrieveKernel.into(),
                CookbookPerm::Requirements.into(),
                CookbookPerm::NetConfigurations.into(),
                CookbookPerm::ListBabelVersions.into()
            }),
            Endpoint::CookbookRetrievePlugin => CookbookPerm::RetrievePlugin.into(),
            Endpoint::CookbookRetrieveImage => CookbookPerm::RetrieveImage.into(),
            Endpoint::CookbookRetrieveKernel => CookbookPerm::RetrieveKernel.into(),
            Endpoint::CookbookRequirements => CookbookPerm::Requirements.into(),
            Endpoint::CookbookNetConfigurations => CookbookPerm::NetConfigurations.into(),
            Endpoint::CookbookListBabelVersions => CookbookPerm::ListBabelVersions.into(),

            Endpoint::BundleAll => Perms::Many(hashset! {
                BundlePerm::Retrieve.into(),
                BundlePerm::ListBundleVersions.into(),
                BundlePerm::Delete.into()
            }),
            Endpoint::BundleRetrieve => BundlePerm::Retrieve.into(),
            Endpoint::BundleListBundleVersions => BundlePerm::ListBundleVersions.into(),
            Endpoint::BundleDelete => BundlePerm::Delete.into(),

            Endpoint::ManifestAll => Perms::Many(hashset! {
                ManifestPerm::RetrieveDownload.into()
            }),
            Endpoint::ManifestRetrieveDownload => ManifestPerm::RetrieveDownload.into(),

            Endpoint::ApiKeyAll => Perms::Many(hashset! {
                ApiKeyPerm::Create.into(),
                ApiKeyPerm::List.into(),
                ApiKeyPerm::Update.into(),
                ApiKeyPerm::Regenerate.into(),
                ApiKeyPerm::Delete.into()
            }),
            Endpoint::ApiKeyCreate => ApiKeyPerm::Create.into(),
            Endpoint::ApiKeyList => ApiKeyPerm::List.into(),
            Endpoint::ApiKeyUpdate => ApiKeyPerm::Update.into(),
            Endpoint::ApiKeyRegenerate => ApiKeyPerm::Regenerate.into(),
            Endpoint::ApiKeyDelete => ApiKeyPerm::Delete.into(),

            Endpoint::SubscriptionAll => Perms::Many(hashset! {
                SubscriptionPerm::Create.into(),
                SubscriptionPerm::Get.into(),
                SubscriptionPerm::List.into(),
                SubscriptionPerm::Delete.into()
            }),
            Endpoint::SubscriptionCreate => SubscriptionPerm::Create.into(),
            Endpoint::SubscriptionGet => SubscriptionPerm::Get.into(),
            Endpoint::SubscriptionList => SubscriptionPerm::List.into(),
            Endpoint::SubscriptionDelete => SubscriptionPerm::Delete.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Endpoints {
    Single(Endpoint),
    Multiple(Vec<Endpoint>),
}

impl Endpoints {
    pub fn includes(&self, endpoint: Endpoint) -> bool {
        match self {
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

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    #[test]
    fn endpoint_matches() {
        use super::Endpoint::*;
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
