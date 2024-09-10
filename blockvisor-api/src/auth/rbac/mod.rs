//! This module lists the RBAC roles and permissions used by blockvisor-api.
//!
//! All roles and permissions are serialized to the database in kebab-case form,
//! minus the implied `Role` or `Perm` suffix.
//!
//! For example, `ApiKeyRole::User` becomes `api-key-user` in the `roles` table.

pub mod access;
pub use access::{Access, Perms, Roles};

#[macro_use]
mod macros;

define_roles! {
    ApiKey => {
        User,
        Org,
        Host,
        Node,
    }

    Blockjoy => {
        Admin,
    }

    Email => {
        Invitation,
        RegistrationConfirmation,
        ResetPassword,
    }

    Grpc => {
        Login,
        NewHost,
    }

    Org => {
        Admin,
        Member,
        Owner,
        Personal,
    }

    View => {
        DeveloperPreview,
    }
}

define_perms! {
    ApiKey => {
        Create,
        List,
        Update,
        Regenerate,
        Delete,
    }

    Archive => {
        GetDownloadMetadata,
        GetDownloadChunks,
        GetUploadSlots,
        PutDownloadManifest,
    }

    ArchiveAdmin => {
        GetDownloadMetadata,
        GetDownloadChunks,
        GetUploadSlots,
        PutDownloadManifest,
    }

    Auth => {
        Confirm,
        ListPermissions,
        Refresh,
        ResetPassword,
        UpdatePassword,
        UpdateUiPassword,
    }

    AuthAdmin => {
        ListPermissions,
    }

    Billing => {
        Exempt,
    }

    Bundle => {
        ListVersions,
        Retrieve,
    }

    Command => {
        Ack,
        Create,
        Get,
        List,
        Pending,
        Update,
    }

    CommandAdmin => {
        List,
        Pending,
    }

    Crypt => {
        GetSecret,
        PutSecret,
    }

    Discovery => {
        Services,
    }

    Host => {
        Delete,
        Get,
        List,
        Regions,
        Restart,
        Start,
        Stop,
        Update,
    }

    HostAdmin => {
        Get,
        List,
        Regions,
        Update,
    }

    HostBilling => {
        Get
    }

    HostProvision => {
        Create,
        Get,
    }

    Image => {
        Get,
        ListArchives,
    }

    ImageAdmin => {
        Add,
        Get,
        ListArchives,
        UpdateArchive,
        UpdateImage,
    }

    Invitation => {
        Create,
        List,
        Accept,
        Decline,
        Revoke,
    }

    InvitationAdmin => {
        Create,
        List,
        Revoke,
    }

    Metrics => {
        Host,
        Node,
    }

    Mqtt => {
        Acl,
    }

    MqttAdmin => {
        Acl,
    }

    Node => {
        Create,
        Delete,
        Get,
        List,
        ReportError,
        ReportStatus,
        Restart,
        Start,
        Stop,
        UpdateConfig,
        Upgrade,
    }

    NodeAdmin => {
        Create,
        Delete,
        Get,
        List,
        ReportError,
        ReportStatus,
        Restart,
        Start,
        Stop,
        Transfer,
        UpdateConfig,
        Upgrade,
    }

    Org => {
        Create,
        Get,
        List,
        Update,
        Delete,
        RemoveMember,
        RemoveSelf,
    }

    OrgAdmin => {
        Get,
        List,
        Update,
    }

    OrgProvision => {
        GetToken,
        ResetToken,
    }

    OrgBilling => {
        GetBillingDetails,
        InitCard,
        ListPaymentMethods,
    }

    OrgAddress => {
        Get,
        Set,
        Delete,
    }

    Protocol => {
        GetProtocol,
        GetLatest,
        GetPricing,
        GetStats,
        ListProtocols,
        ListVersions,
        ViewDevelopment,
        ViewPublic,
    }

    ProtocolAdmin => {
        AddProtocol,
        AddVersion,
        GetProtocol,
        GetLatest,
        ListProtocols,
        ListVersions,
        UpdateProtocol,
        UpdateVersion,
        ViewAllStats,
        ViewPrivate,
    }

    User => {
        Create,
        Filter,
        Get,
        Update,
        Delete,
    }

    UserAdmin => {
        Filter,
        Get,
        Update,
    }

    UserSettings => {
        Get,
        Update,
        Delete,
    }

    UserSettingsAdmin => {
        Get,
        Update,
        Delete,
    }
}
