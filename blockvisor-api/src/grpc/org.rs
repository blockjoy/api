use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{debug, error};

use crate::auth::rbac::{OrgAdminPerm, OrgPerm, OrgProvisionPerm};
use crate::auth::resource::{OrgId, UserId};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::org::{NewOrg, OrgFilter, OrgSearch, OrgSort, UpdateOrg};
use crate::models::rbac::{OrgUsers, RbacUser};
use crate::models::{Invitation, Org, OrgUser, User};
use crate::util::{HashVec, NanosUtc};

use super::api::org_service_server::OrgService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// No org found after conversion.
    ConvertNoOrg,
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Claims Resource is not a user.
    ClaimsNotUser,
    /// Can't delete personal org.
    DeletePersonal,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Org invitation error: {0}
    Invitation(#[from] crate::models::invitation::Error),
    /// Failed to parse member count: {0}
    MemberCount(std::num::TryFromIntError),
    /// Missing permission: org-remove-self
    MissingRemoveSelf,
    /// Org model error: {0}
    Model(#[from] crate::models::org::Error),
    /// Failed to parse `id` as OrgId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// Org rbac error: {0}
    Rbac(#[from] crate::models::rbac::Error),
    /// Org resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Cannot remove last owner from an org.
    RemoveLastOwner,
    /// Org search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// Org user error: {0}
    User(#[from] crate::models::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser | DeletePersonal | MissingRemoveSelf => {
                Status::permission_denied("Access denied.")
            }
            ConvertNoOrg | Diesel(_) | MemberCount(_) => Status::internal("Internal error."),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            RemoveLastOwner => Status::failed_precondition("Can't remove last org owner."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Invitation(err) => err.into(),
            Model(err) => err.into(),
            Rbac(err) => err.into(),
            Resource(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl OrgService for Grpc {
    async fn create(
        &self,
        req: Request<api::OrgServiceCreateRequest>,
    ) -> Result<Response<api::OrgServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::OrgServiceGetRequest>,
    ) -> Result<Response<api::OrgServiceGetResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta, read).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::OrgServiceListRequest>,
    ) -> Result<Response<api::OrgServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: Request<api::OrgServiceUpdateRequest>,
    ) -> Result<Response<api::OrgServiceUpdateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta, write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::OrgServiceDeleteRequest>,
    ) -> Result<Response<api::OrgServiceDeleteResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta, write).scope_boxed())
            .await
    }

    async fn remove_member(
        &self,
        req: Request<api::OrgServiceRemoveMemberRequest>,
    ) -> Result<Response<api::OrgServiceRemoveMemberResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| remove_member(req, meta, write).scope_boxed())
            .await
    }

    async fn get_provision_token(
        &self,
        req: Request<api::OrgServiceGetProvisionTokenRequest>,
    ) -> Result<Response<api::OrgServiceGetProvisionTokenResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_provision_token(req, meta, read).scope_boxed())
            .await
    }

    async fn reset_provision_token(
        &self,
        req: Request<api::OrgServiceResetProvisionTokenRequest>,
    ) -> Result<Response<api::OrgServiceResetProvisionTokenResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| reset_provision_token(req, meta, write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::OrgServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceCreateResponse, Error> {
    let authz = write.auth_all(&meta, OrgPerm::Create).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;
    let user = User::by_id(user_id, &mut write).await?;

    let new_org = NewOrg {
        name: &req.name,
        is_personal: false,
    };
    let org = new_org.create(user.id, &mut write).await?;
    let org = api::Org::from_model(&org, &mut write).await?;

    let created_by = common::EntityUpdate::from_user(&user);
    let msg = api::OrgMessage::created(org.clone(), created_by);
    write.mqtt(msg);

    Ok(api::OrgServiceCreateResponse { org: Some(org) })
}

async fn get(
    req: api::OrgServiceGetRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetResponse, Error> {
    let org_id: OrgId = req.id.parse().map_err(Error::ParseId)?;
    read.auth_or_all(&meta, OrgAdminPerm::Get, OrgPerm::Get, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut read).await?;
    let org = api::Org::from_model(&org, &mut read).await?;

    Ok(api::OrgServiceGetResponse { org: Some(org) })
}

async fn list(
    req: api::OrgServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceListResponse, Error> {
    let filter = req.into_filter()?;
    if let Some(user_id) = filter.member_id {
        read.auth(&meta, OrgPerm::List, user_id).await?
    } else {
        read.auth_all(&meta, OrgAdminPerm::List).await?
    };

    let (orgs, org_count) = filter.query(&mut read).await?;
    let orgs = api::Org::from_models(&orgs, &mut read).await?;

    Ok(api::OrgServiceListResponse { orgs, org_count })
}

async fn update(
    req: api::OrgServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceUpdateResponse, Error> {
    let org_id: OrgId = req.id.parse().map_err(Error::ParseId)?;
    let authz = write.auth(&meta, OrgPerm::Update, org_id).await?;

    let update = UpdateOrg {
        id: org_id,
        name: req.name.as_deref(),
    };
    let org = update.update(&mut write).await?;
    let org = api::Org::from_model(&org, &mut write).await?;

    let updated_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let msg = api::OrgMessage::updated(org, updated_by);
    write.mqtt(msg);

    Ok(api::OrgServiceUpdateResponse {})
}

async fn delete(
    req: api::OrgServiceDeleteRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceDeleteResponse, Error> {
    let org_id: OrgId = req.id.parse().map_err(Error::ParseId)?;
    let authz = write.auth(&meta, OrgPerm::Delete, org_id).await?;

    let org = Org::by_id(org_id, &mut write).await?;
    if org.is_personal {
        return Err(Error::DeletePersonal);
    }

    debug!("Deleting org: {org_id}");
    org.delete(&mut write).await?;

    let invitations = Invitation::by_org_id(org.id, &mut write).await?;
    let invitation_ids = invitations.into_iter().map(|i| i.id).collect();
    Invitation::bulk_delete(invitation_ids, &mut write).await?;

    let deleted_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let msg = api::OrgMessage::deleted(&org, deleted_by);
    write.mqtt(msg);

    Ok(api::OrgServiceDeleteResponse {})
}

async fn remove_member(
    req: api::OrgServiceRemoveMemberRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceRemoveMemberResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let authz = write.auth(&meta, OrgPerm::RemoveMember, org_id).await?;

    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;
    let user = User::by_id(user_id, &mut write).await?;

    if let Some(self_id) = authz.resource().user() {
        if user_id == self_id && !authz.has_perm(OrgPerm::RemoveSelf) {
            return Err(Error::MissingRemoveSelf);
        }
    }

    let org = Org::by_id(org_id, &mut write).await?;
    if org.is_personal {
        return Err(Error::DeletePersonal);
    }

    let owners = RbacUser::org_owners(org_id, &mut write).await?;
    if owners.len() == 1 && owners[0] == user_id {
        return Err(Error::RemoveLastOwner);
    }

    org.remove_user(user_id, &mut write).await?;

    // In case a user needs to be re-invited later, we also remove the (already accepted) invites
    // from the database. This is to prevent them from running into a unique constraint when they
    // are invited again.
    Invitation::remove_by_org_user(&user.email, org_id, &mut write).await?;

    let org = api::Org::from_model(&org, &mut write).await?;
    let updated_by = common::EntityUpdate::from_resource(&authz, &mut write).await?;
    let msg = api::OrgMessage::updated(org, updated_by);
    write.mqtt(msg);

    Ok(api::OrgServiceRemoveMemberResponse {})
}

async fn get_provision_token(
    req: api::OrgServiceGetProvisionTokenRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetProvisionTokenResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, OrgProvisionPerm::GetToken, org_id).await?;

    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let org_user = OrgUser::by_user_org(user_id, org_id, &mut read).await?;

    Ok(api::OrgServiceGetProvisionTokenResponse {
        token: org_user.host_provision_token,
    })
}

async fn reset_provision_token(
    req: api::OrgServiceResetProvisionTokenRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceResetProvisionTokenResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write
        .auth(&meta, OrgProvisionPerm::ResetToken, org_id)
        .await?;

    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let org_user = OrgUser::by_user_org(user_id, org_id, &mut write).await?;
    let token = org_user.reset_token(&mut write).await?;

    Ok(api::OrgServiceResetProvisionTokenResponse { token })
}

impl api::Org {
    /// Converts a list of `orgs` into a list of `api::Org`.
    ///
    /// Performs O(1) database queries irrespective of the number of orgs.
    pub async fn from_models<O>(orgs: &[O], conn: &mut Conn<'_>) -> Result<Vec<Self>, Error>
    where
        O: AsRef<Org> + Send + Sync,
    {
        let org_ids = orgs
            .iter()
            .map(|org| org.as_ref().id)
            .collect::<HashSet<_>>();

        let host_counts = Org::host_counts(&org_ids, conn).await?;
        let node_counts = Org::node_counts(&org_ids, conn).await?;

        let mut org_users = OrgUsers::for_org_ids(org_ids, conn).await?;

        let user_ids = org_users
            .values()
            .flat_map(|ou| ou.user_roles.keys().copied())
            .collect();
        let users = User::by_ids(user_ids, conn)
            .await?
            .to_map_keep_last(|u| (u.id, u));

        orgs.iter()
            .map(|org| {
                let org = org.as_ref();
                let org_users = org_users
                    .remove(&org.id)
                    .unwrap_or_else(|| OrgUsers::empty(org.id));
                let members: Vec<_> = org_users
                    .user_roles
                    .iter()
                    .filter_map(|(user_id, roles)| {
                        users.get(user_id).map(|user| api::OrgUser {
                            user_id: user_id.to_string(),
                            org_id: org.id.to_string(),
                            name: user.name(),
                            email: user.email.clone(),
                            roles: roles
                                .iter()
                                .map(|role| api::OrgRole {
                                    name: Some(role.to_string()),
                                })
                                .collect(),
                        })
                    })
                    .collect();

                Ok(api::Org {
                    id: org.id.to_string(),
                    name: org.name.clone(),
                    personal: org.is_personal,
                    created_at: Some(NanosUtc::from(org.created_at).into()),
                    updated_at: Some(NanosUtc::from(org.updated_at).into()),
                    member_count: members.len().try_into().map_err(Error::MemberCount)?,
                    members,
                    host_count: host_counts.get(&org.id).copied().unwrap_or(0),
                    node_count: node_counts.get(&org.id).copied().unwrap_or(0),
                })
            })
            .collect()
    }

    pub async fn from_model(org: &Org, conn: &mut Conn<'_>) -> Result<Self, Error> {
        Self::from_models(&[org], conn)
            .await?
            .pop()
            .ok_or(Error::ConvertNoOrg)
    }
}

impl api::OrgServiceListRequest {
    fn into_filter(self) -> Result<OrgFilter, Error> {
        let member_id = self
            .member_id
            .map(|id| id.parse().map_err(Error::ParseUserId))
            .transpose()?;
        let search = self
            .search
            .map(|search| {
                Ok::<_, Error>(OrgSearch {
                    operator: search
                        .operator()
                        .try_into()
                        .map_err(Error::SearchOperator)?,
                    id: search.id.map(|id| id.trim().to_lowercase()),
                    name: search.name.map(|name| name.trim().to_lowercase()),
                })
            })
            .transpose()?;
        let sort = self
            .sort
            .into_iter()
            .map(|sort| {
                let order = sort.order().try_into().map_err(Error::SortOrder)?;
                match sort.field() {
                    api::OrgSortField::Unspecified => Err(Error::UnknownSortField),
                    api::OrgSortField::Name => Ok(OrgSort::Name(order)),
                    api::OrgSortField::CreatedAt => Ok(OrgSort::CreatedAt(order)),
                    api::OrgSortField::UpdatedAt => Ok(OrgSort::UpdatedAt(order)),
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(OrgFilter {
            member_id,
            personal: self.personal,
            offset: self.offset,
            limit: self.limit,
            search,
            sort,
        })
    }
}
