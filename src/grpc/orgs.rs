use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;

use super::api::{self, org_service_server};
use crate::auth::token::{Endpoint, Resource};
use crate::{auth, models};

struct OrgResult<T> {
    org_msg: api::OrgMessage,
    resp: tonic::Response<T>,
}

#[tonic::async_trait]
impl org_service_server::OrgService for super::GrpcImpl {
    async fn create(
        &self,
        req: tonic::Request<api::OrgServiceCreateRequest>,
    ) -> super::Resp<api::OrgServiceCreateResponse> {
        let result = self.trx(|c| create(req, c).scope_boxed()).await?;
        self.notifier.orgs_sender().send(&result.org_msg).await?;
        Ok(result.resp)
    }

    async fn get(
        &self,
        req: tonic::Request<api::OrgServiceGetRequest>,
    ) -> super::Resp<api::OrgServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn list(
        &self,
        req: tonic::Request<api::OrgServiceListRequest>,
    ) -> super::Resp<api::OrgServiceListResponse> {
        let mut conn = self.conn().await?;
        let resp = list(req, &mut conn).await?;
        Ok(resp)
    }

    async fn update(
        &self,
        req: tonic::Request<api::OrgServiceUpdateRequest>,
    ) -> super::Resp<api::OrgServiceUpdateResponse> {
        let result = self.trx(|c| update(req, c).scope_boxed()).await?;
        self.notifier.orgs_sender().send(&result.org_msg).await?;
        Ok(result.resp)
    }

    async fn delete(
        &self,
        req: tonic::Request<api::OrgServiceDeleteRequest>,
    ) -> super::Resp<api::OrgServiceDeleteResponse> {
        let result = self.trx(|c| delete(req, c).scope_boxed()).await?;
        self.notifier.orgs_sender().send(&result.org_msg).await?;
        Ok(result.resp)
    }

    async fn remove_member(
        &self,
        req: tonic::Request<api::OrgServiceRemoveMemberRequest>,
    ) -> super::Resp<api::OrgServiceRemoveMemberResponse> {
        let result = self.trx(|c| remove_member(req, c).scope_boxed()).await?;
        self.notifier.orgs_sender().send(&result.org_msg).await?;
        Ok(result.resp)
    }

    async fn get_provision_token(
        &self,
        req: tonic::Request<api::OrgServiceGetProvisionTokenRequest>,
    ) -> super::Resp<api::OrgServiceGetProvisionTokenResponse> {
        let mut conn = self.conn().await?;
        let resp = get_provision_token(req, &mut conn).await?;
        Ok(resp)
    }

    async fn reset_provision_token(
        &self,
        req: tonic::Request<api::OrgServiceResetProvisionTokenRequest>,
    ) -> super::Resp<api::OrgServiceResetProvisionTokenResponse> {
        let mut conn = self.conn().await?;
        let resp = reset_provision_token(req, &mut conn).await?;
        Ok(resp)
    }
}

async fn create(
    req: tonic::Request<api::OrgServiceCreateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<OrgResult<api::OrgServiceCreateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::OrgCreate, conn).await?;
    let req = req.into_inner();
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Access denied") };
    let new_org = models::NewOrg {
        name: &req.name,
        is_personal: false,
    };
    let user = models::User::find_by_id(user_id, conn).await?;
    let org = new_org.create(user.id, conn).await?;
    let org = api::Org::from_model(org.clone(), conn).await?;
    let msg = api::OrgMessage::created(org.clone(), user);
    let resp = api::OrgServiceCreateResponse { org: Some(org) };
    Ok(OrgResult {
        resp: tonic::Response::new(resp),
        org_msg: msg,
    })
}

async fn get(
    req: tonic::Request<api::OrgServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::OrgServiceGetResponse> {
    let claims = auth::get_claims(&req, Endpoint::OrgGet, conn).await?;
    let req = req.into_inner();
    let org_id = req.id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, org_id, conn).await?,
        Resource::Org(org) => org == org_id,
        Resource::Host(host) => models::Host::find_by_id(host, conn).await?.org_id == org_id,
        Resource::Node(node) => models::Node::find_by_id(node, conn).await?.org_id == org_id,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let org = models::Org::find_by_id(org_id, conn).await?;
    let org = api::Org::from_model(org, conn).await?;
    let resp = api::OrgServiceGetResponse { org: Some(org) };

    Ok(tonic::Response::new(resp))
}

async fn list(
    req: tonic::Request<api::OrgServiceListRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::OrgServiceListResponse> {
    let claims = auth::get_claims(&req, Endpoint::OrgList, conn).await?;
    let req = req.into_inner();
    let member_id = req.member_id.map(|id| id.parse()).transpose()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            if let Some(member_id) = member_id {
                member_id == user_id
            } else {
                false
            }
        }
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied")
    }
    let orgs = models::Org::filter(member_id, conn).await?;
    let orgs = api::Org::from_models(orgs, conn).await?;
    let resp = api::OrgServiceListResponse { orgs };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::OrgServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<OrgResult<api::OrgServiceUpdateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::OrgUpdate, conn).await?;
    let req = req.into_inner();
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Access denied") };
    let org_id = req.id.parse()?;
    if !models::Org::is_member(user_id, org_id, conn).await? {
        super::forbidden!("Access denied");
    }
    let update = models::UpdateOrg {
        id: org_id,
        name: req.name.as_deref(),
    };
    let org_model = update.update(conn).await?;
    let user = models::User::find_by_id(user_id, conn).await?;
    let org = api::Org::from_model(org_model, conn).await?;
    let msg = api::OrgMessage::updated(org, user);
    let resp = api::OrgServiceUpdateResponse {};
    Ok(OrgResult {
        resp: tonic::Response::new(resp),
        org_msg: msg,
    })
}

async fn delete(
    req: tonic::Request<api::OrgServiceDeleteRequest>,
    conn: &mut models::Conn,
) -> crate::Result<OrgResult<api::OrgServiceDeleteResponse>> {
    let claims = auth::get_claims(&req, Endpoint::OrgDelete, conn).await?;
    let req = req.into_inner();
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Access denied") };
    let org_id = req.id.parse()?;
    if !models::Org::is_admin(user_id, org_id, conn).await? {
        super::forbidden!("User {user_id} has insufficient privileges to delete org {org_id}");
    }
    let org = models::Org::find_by_id(org_id, conn).await?;
    if org.is_personal {
        super::forbidden!("Can't deleted personal org");
    }

    tracing::debug!("Deleting org: {}", org_id);
    org.delete(conn).await?;
    let user = models::User::find_by_id(user_id, conn).await?;
    let msg = api::OrgMessage::deleted(org, user);
    let resp = api::OrgServiceDeleteResponse {};
    Ok(OrgResult {
        resp: tonic::Response::new(resp),
        org_msg: msg,
    })
}

async fn remove_member(
    req: tonic::Request<api::OrgServiceRemoveMemberRequest>,
    conn: &mut models::Conn,
) -> crate::Result<OrgResult<api::OrgServiceRemoveMemberResponse>> {
    let claims = auth::get_claims(&req, Endpoint::OrgRemoveMember, conn).await?;
    let req = req.into_inner();
    let Resource::User(caller_id) = claims.resource() else { super::forbidden!("Access denied") };
    let org_id = req.org_id.parse()?;
    let user_id = req.user_id.parse()?;
    let is_admin = models::Org::is_admin(caller_id, org_id, conn).await?;
    let is_self = caller_id == user_id;
    if !is_admin && !is_self {
        super::forbidden!("User {caller_id} has can't remove user {user_id} from org {org_id}")
    }
    let user_to_remove = models::User::find_by_id(user_id, conn).await?;
    let org = models::Org::find_by_id(org_id, conn).await?;
    org.remove_member(&user_to_remove, conn).await?;
    // In case a user needs to be re-invited later, we also remove the (already accepted) invites
    // from the database. This is to prevent them from running into a unique constraint when they
    // are invited again.
    models::Invitation::remove_by_org_user(&user_to_remove.email, org_id, conn).await?;
    let org_model = models::Org::find_by_id(org_id, conn).await?;
    let user = models::User::find_by_id(caller_id, conn).await?;
    let org = api::Org::from_model(org_model, conn).await?;
    let msg = api::OrgMessage::updated(org, user);
    let resp = api::OrgServiceRemoveMemberResponse {};
    Ok(OrgResult {
        resp: tonic::Response::new(resp),
        org_msg: msg,
    })
}

async fn get_provision_token(
    req: tonic::Request<api::OrgServiceGetProvisionTokenRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::OrgServiceGetProvisionTokenResponse> {
    let claims = auth::get_claims(&req, Endpoint::OrgGetProvisionToken, conn).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let org_id = req.org_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id_) => {
            user_id_ == user_id && models::Org::is_member(user_id, org_id, conn).await?
        }
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let org_user = models::OrgUser::by_user_org(user_id, org_id, conn).await?;
    let resp = api::OrgServiceGetProvisionTokenResponse {
        token: org_user.host_provision_token,
    };
    Ok(tonic::Response::new(resp))
}

async fn reset_provision_token(
    req: tonic::Request<api::OrgServiceResetProvisionTokenRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::OrgServiceResetProvisionTokenResponse> {
    let claims = auth::get_claims(&req, Endpoint::OrgResetProvisionToken, conn).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let org_id = req.org_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id_) => {
            user_id_ == user_id && models::Org::is_member(user_id, org_id, conn).await?
        }
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let org_user = models::OrgUser::by_user_org(user_id, org_id, conn).await?;
    let token = org_user.reset_token(conn).await?;
    let resp = api::OrgServiceResetProvisionTokenResponse { token };
    Ok(tonic::Response::new(resp))
}

impl api::Org {
    /// Converts a list of `models::Org` into a list of `api::Org`. We take care to perform O(1)
    /// queries, no matter the length of `models`. For this we need to find all users belonging to
    /// this each org.
    pub async fn from_models(
        models: Vec<models::Org>,
        conn: &mut models::Conn,
    ) -> crate::Result<Vec<Self>> {
        // We find all OrgUsers belonging to each model. This gives us a map from `org_id` to
        // `Vec<OrgUser>`.
        let org_users = models::OrgUser::by_orgs(&models, conn).await?;

        // Now we get the actual users for each `OrgUser`, because we also need to provide the name
        // and email of each user.
        let user_ids: Vec<uuid::Uuid> = org_users.values().flatten().map(|ou| ou.user_id).collect();
        let users: HashMap<uuid::Uuid, models::User> = models::User::find_by_ids(&user_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let node_counts = models::Org::node_counts(&models, conn).await?;

        // Finally we can loop over the models to construct the final list of messages we set out to
        // create.
        models
            .into_iter()
            .map(|model| {
                let empty = vec![];
                let org_users = org_users.get(&model.id).unwrap_or(&empty);
                Ok(Self {
                    id: model.id.to_string(),
                    name: model.name.clone(),
                    personal: model.is_personal,
                    member_count: org_users.len().try_into()?,
                    created_at: Some(super::try_dt_to_ts(model.created_at)?),
                    updated_at: Some(super::try_dt_to_ts(model.updated_at)?),
                    members: org_users
                        .iter()
                        .flat_map(|ou| {
                            // When a user gets deleted, we might not have a user for the current id
                            // so we flat_map here and skip any user that don't exist.
                            users.get(&ou.user_id).map(|user| {
                                let mut org = api::OrgUser {
                                    user_id: ou.user_id.to_string(),
                                    org_id: ou.org_id.to_string(),
                                    role: 0, // We use the setter to set this field for type-safety
                                    name: user.name(),
                                    email: user.email.clone(),
                                };
                                org.set_role(api::OrgRole::from_model(ou.role));
                                org
                            })
                        })
                        .collect(),
                    node_count: node_counts.get(&model.id).copied().unwrap_or(0),
                })
            })
            .collect()
    }

    pub async fn from_model(model: models::Org, conn: &mut models::Conn) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], conn).await?[0].clone())
    }
}

impl api::OrgRole {
    fn from_model(model: models::OrgRole) -> Self {
        match model {
            models::OrgRole::Admin => Self::Admin,
            models::OrgRole::Owner => Self::Owner,
            models::OrgRole::Member => Self::Member,
        }
    }
}
