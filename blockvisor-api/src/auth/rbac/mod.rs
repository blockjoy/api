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
}

define_perms! {
    ApiKey => {
        Create,
        List,
        Update,
        Regenerate,
        Delete,
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

    Babel => {
        Notify,
    }

    Blockchain => {
        Get,
        GetImage,
        GetPlugin,
        GetRequirements,
        List,
        ListImageVersions,
    }

    BlockchainAdmin => {
        AddNodeType,
        AddVersion,
        Get,
        List,
    }

    BlockchainArchive => {
        GetDownload,
        GetUpload,
        PutDownload,
    }

    Bundle => {
        Retrieve,
        ListBundleVersions,
        Delete,
    }

    Command => {
        Create,
        Get,
        Update,
        Pending,
        Ack,
    }

    Discovery => {
        Services,
    }

    Host => {
        Create,
        Get,
        List,
        Update,
        Delete,
        Start,
        Stop,
        Restart,
        Regions,
    }

    HostAdmin => {
        Get,
        List,
    }

    HostBilling => {
        Get
    }

    HostProvision => {
        Get,
        Create,
    }

    Invitation => {
        Create,
        List,
        Accept,
        Decline,
        Revoke,
    }

    Kernel => {
        Retrieve,
    }

    KeyFile => {
        Create,
        List,
    }

    Metrics => {
        Node,
        Host,
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
        Report,
        Restart,
        Start,
        Stop,
        UpdateConfig,
        UpdateStatus,
    }

    NodeAdmin => {
        Create,
        Delete,
        Get,
        List,
        Report,
        Restart,
        Start,
        Stop,
        UpdateConfig,
        UpdateStatus,
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
    }

    OrgProvision => {
        GetToken,
        ResetToken,
    }

    Subscription => {
        Create,
        Get,
        List,
        Update,
        Delete,
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

    UserBilling => {
        Get,
        Update,
        Delete,
    }
}
