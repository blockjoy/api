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
        Refresh,
        ResetPassword,
        UpdatePassword,
        UpdateUiPassword,
    }

    Babel => {
        Notify,
    }

    Blockchain => {
        List,
        Get,
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

    Cookbook => {
        RetrievePlugin,
        RetrieveImage,
        RetrieveKernel,
        Requirements,
        NetConfigurations,
        ListBabelVersions,
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

    KeyFile => {
        Create,
        List,
    }

    Manifest => {
        RetrieveDownload,
    }

    Metrics => {
        Node,
        Host,
    }

    Mqtt => {
        Acl,
    }

    Node => {
        Create,
        Get,
        List,
        UpdateConfig,
        Delete,
        UpdateStatus,
        Start,
        Stop,
        Restart,
    }

    NodeAdmin => {
        CreateAll,
        DeleteAll,
        ListAll,
        RestartAll,
        StartAll,
        StopAll,
        UpdateConfigAll,
        UpdateStatusAll,
    }

    Org => {
        Create,
        Get,
        List,
        Update,
        Delete,
        RemoveMember,
    }

    OrgAdmin => {
        ListAll,
    }

    OrgProvision => {
        GetToken,
        ResetToken,
    }

    Subscription => {
        Create,
        Get,
        List,
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
        FilterAll,
        UpdateAll,
    }

    UserBilling => {
        Get,
        Update,
        Delete,
    }
}
