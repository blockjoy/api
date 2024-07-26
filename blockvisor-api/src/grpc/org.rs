use std::cmp::max;
use std::collections::HashSet;
use std::num::TryFromIntError;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use futures::future::OptionFuture;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{debug, error};

use crate::auth::rbac::{
    OrgAddressPerm, OrgAdminPerm, OrgBillingPerm, OrgPerm, OrgProvisionPerm, OrgRole, Role,
};
use crate::auth::resource::{OrgId, UserId};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::models::org::{NewOrg, OrgFilter, OrgSearch, OrgSort, UpdateOrg};
use crate::models::rbac::{OrgUsers, RbacUser};
use crate::models::{Address, Invitation, NewAddress, Org, Token, User};
use crate::util::{HashVec, NanosUtc};

use super::api::org_service_server::OrgService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Address error: {0}
    Address(#[from] crate::models::address::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Failed to remove user from org with permission `org-remove-self`, user to remove is not self
    CanOnlyRemoveSelf,
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
    /// The request is missing the `address` fields.
    MissingAddress,
    /// Org model error: {0}
    Model(#[from] crate::models::org::Error),
    /// Negative price encountered:
    NegativePrice(TryFromIntError),
    /// Org `{0}` has no owner.
    NoOwner(OrgId),
    /// No customer exists in stripe for org `{0}`.
    NoStripeCustomer(OrgId),
    /// No subscription exists in stripe for org `{0}`.
    NoStripeSubscription(OrgId),
    /// Failed to parse `id` as OrgId: {0}
    ParseId(uuid::Error),
    /// Failed to parse non-zero count as u64: {0}
    ParseMax(std::num::TryFromIntError),
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
    /// Stripe error: {0}
    Stripe(#[from] crate::stripe::Error),
    /// Org token error: {0}
    Token(#[from] crate::models::token::Error),
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
            ClaimsNotUser | DeletePersonal | CanOnlyRemoveSelf => {
                Status::permission_denied("Access denied.")
            }
            ConvertNoOrg | Diesel(_) | ParseMax(_) | Stripe(_) => {
                Status::internal("Internal error.")
            }
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            RemoveLastOwner => Status::failed_precondition("Can't remove last org owner."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            MissingAddress => Status::failed_precondition("User has no address."),
            NegativePrice(_) => Status::internal("Negative price encountered"),
            NoOwner(_) => Status::failed_precondition("Org has no owner."),
            NoStripeCustomer(_) => Status::failed_precondition("No customer for that org."),
            NoStripeSubscription(_) => Status::failed_precondition("No subscription for that org."),
            Address(err) => err.into(),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Invitation(err) => err.into(),
            Model(err) => err.into(),
            Rbac(err) => err.into(),
            Resource(err) => err.into(),
            Token(err) => err.into(),
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

    async fn init_card(
        &self,
        req: Request<api::OrgServiceInitCardRequest>,
    ) -> Result<Response<api::OrgServiceInitCardResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| init_card(req, meta, write).scope_boxed())
            .await
    }

    async fn list_payment_methods(
        &self,
        req: Request<api::OrgServiceListPaymentMethodsRequest>,
    ) -> Result<Response<api::OrgServiceListPaymentMethodsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_payment_methods(req, meta, read).scope_boxed())
            .await
    }

    async fn billing_details(
        &self,
        req: Request<api::OrgServiceBillingDetailsRequest>,
    ) -> Result<Response<api::OrgServiceBillingDetailsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| billing_details(req, meta, read).scope_boxed())
            .await
    }

    async fn get_address(
        &self,
        req: Request<api::OrgServiceGetAddressRequest>,
    ) -> Result<Response<api::OrgServiceGetAddressResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_address(req, meta, read).scope_boxed())
            .await
    }

    async fn set_address(
        &self,
        req: Request<api::OrgServiceSetAddressRequest>,
    ) -> Result<Response<api::OrgServiceSetAddressResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| set_address(req, meta, read).scope_boxed())
            .await
    }

    async fn delete_address(
        &self,
        req: Request<api::OrgServiceDeleteAddressRequest>,
    ) -> Result<Response<api::OrgServiceDeleteAddressResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| delete_address(req, meta, read).scope_boxed())
            .await
    }

    async fn get_invoices(
        &self,
        req: Request<api::OrgServiceGetInvoicesRequest>,
    ) -> Result<Response<api::OrgServiceGetInvoicesResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_invoices(req, meta, read).scope_boxed())
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
    let authz = write
        .auth_or_all(&meta, OrgAdminPerm::Update, OrgPerm::Update, org_id)
        .await?;

    let update = UpdateOrg {
        id: org_id,
        name: req.name.as_deref(),
        address_id: None,
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

    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;
    let user = User::by_id(user_id, &mut write).await?;

    let authz = match write.auth(&meta, OrgPerm::RemoveMember, org_id).await {
        Ok(authz) => authz,
        Err(err) => {
            if let Ok(authz) = write.auth(&meta, OrgPerm::RemoveSelf, org_id).await {
                if Some(user_id) != authz.resource().user() {
                    return Err(Error::CanOnlyRemoveSelf);
                }
                authz
            } else {
                return Err(err.into());
            }
        }
    };

    let org = Org::by_id(org_id, &mut write).await?;
    if org.is_personal {
        return Err(Error::DeletePersonal);
    }

    let owners = RbacUser::org_owners(org_id, &mut write).await?;
    if owners.len() == 1 && owners[0] == user_id {
        return Err(Error::RemoveLastOwner);
    }

    Org::remove_user(user_id, org_id, &mut write).await?;

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
    let token = Token::host_provision_by_user(user_id, org_id, &mut read).await?;

    Ok(api::OrgServiceGetProvisionTokenResponse {
        token: token.token.take(),
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
    let new_token = Token::reset_host_provision(user_id, org_id, &mut write).await?;

    Ok(api::OrgServiceResetProvisionTokenResponse {
        token: new_token.take(),
    })
}

async fn init_card(
    req: api::OrgServiceInitCardRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceInitCardResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseUserId)?;
    write.auth(&meta, OrgBillingPerm::InitCard, org_id).await?;

    let client_secret = write
        .ctx
        .stripe
        .create_setup_intent(org_id, user_id)
        .await?
        .client_secret;

    Ok(api::OrgServiceInitCardResponse { client_secret })
}

async fn list_payment_methods(
    req: api::OrgServiceListPaymentMethodsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceListPaymentMethodsResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, OrgBillingPerm::ListPaymentMethods, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut read).await?;
    let payment_methods = if let Some(customer_id) = &org.stripe_customer_id {
        read.ctx.stripe.list_payment_methods(customer_id).await?
    } else {
        vec![]
    };

    let methods = payment_methods
        .into_iter()
        .map(|pm| api::PaymentMethod {
            org_id: Some(org_id.to_string()),
            user_id: pm.metadata.and_then(|meta| meta.get("user_id").cloned()),
            details: Some(api::BillingDetails {
                address: pm.billing_details.address.map(common::Address::from_stripe),
                email: pm.billing_details.email,
                name: pm.billing_details.name,
                phone: pm.billing_details.phone,
            }),
            created_at: chrono::DateTime::from_timestamp(pm.created.0, 0)
                .map(NanosUtc::from)
                .map(Into::into),
            updated_at: chrono::DateTime::from_timestamp(pm.created.0, 0)
                .map(NanosUtc::from)
                .map(Into::into),
            method: pm.card.map(|card| {
                api::payment_method::Method::Card(api::Card {
                    brand: card.brand,
                    exp_month: card.exp_month,
                    exp_year: card.exp_year,
                    last4: card.last4,
                })
            }),
        })
        .collect();

    Ok(api::OrgServiceListPaymentMethodsResponse { methods })
}

async fn billing_details(
    req: api::OrgServiceBillingDetailsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceBillingDetailsResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, OrgBillingPerm::ListPaymentMethods, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut read).await?;
    let subscription = if let Some(customer_id) = org.stripe_customer_id.as_deref() {
        read.ctx
            .stripe
            .get_subscription(customer_id)
            .await?
            .ok_or_else(|| Error::NoStripeSubscription(org_id))?
    } else {
        return Err(Error::NoStripeCustomer(org_id));
    };

    Ok(api::OrgServiceBillingDetailsResponse {
        currency: common::Currency::from_stripe(subscription.currency)
            .unwrap_or(common::Currency::Usd) as i32,
        current_period_start: chrono::DateTime::from_timestamp(
            subscription.current_period_start.0,
            0,
        )
        .map(NanosUtc::from)
        .map(Into::into),
        current_period_end: chrono::DateTime::from_timestamp(subscription.current_period_end.0, 0)
            .map(NanosUtc::from)
            .map(Into::into),
        default_payment_method: subscription.default_payment_method,
        created_at: chrono::DateTime::from_timestamp(subscription.created.0, 0)
            .map(NanosUtc::from)
            .map(Into::into),
        status: subscription.status.to_string(),
        items: subscription
            .items
            .data
            .into_iter()
            .map(|item| api::BillingItem {
                name: item.price.as_ref().and_then(|price| price.nickname.clone()),
                unit_amount: item.price.as_ref().and_then(|price| price.unit_amount),
                quantity: item.quantity,
            })
            .collect(),
    })
}

async fn get_address(
    req: api::OrgServiceGetAddressRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth(&meta, OrgAddressPerm::Get, org_id).await?;
    let org = Org::by_id(org_id, &mut read).await?;
    let customer_id = org
        .stripe_customer_id
        .as_deref()
        .ok_or(Error::NoStripeCustomer(org_id))?;
    let address = read.ctx.stripe.get_address(customer_id).await?;
    Ok(api::OrgServiceGetAddressResponse {
        address: address.map(Into::into),
    })
}

async fn set_address(
    req: api::OrgServiceSetAddressRequest,
    meta: MetadataMap,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceSetAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write.auth(&meta, OrgAddressPerm::Set, org_id).await?;

    let org = Org::by_id(org_id, &mut write).await?;
    let address = req.address.ok_or(Error::MissingAddress)?;
    let (org, customer_id) = if let Some(customer_id) = org.stripe_customer_id.clone() {
        (org, customer_id)
    } else {
        let owner = User::by_org_role(org_id, Role::Org(OrgRole::Owner), &mut write)
            .await?
            .pop()
            .ok_or_else(|| Error::NoOwner(org_id))?;
        let customer_id = write
            .ctx
            .stripe
            .create_customer(&org, &owner, None)
            .await?
            .id;
        let org = org.set_customer_id(&customer_id, &mut write).await?;
        (org, customer_id)
    };
    let address = write
        .ctx
        .stripe
        .set_address(&customer_id, &address.into())
        .await?;
    let maybe_address = org.address_id.map(|a_id| Address::by_id(a_id, &mut write));
    match OptionFuture::from(maybe_address).await {
        Some(Ok(mut existing)) => {
            existing.city = address.city;
            existing.country = address.country;
            existing.line1 = address.line1;
            existing.line2 = address.line2;
            existing.postal_code = address.postal_code;
            existing.state = address.state;
            existing.update(&mut write).await?;
        }
        None
        | Some(Err(crate::models::address::Error::FindById(_, diesel::result::Error::NotFound))) => {
            let new_address = NewAddress::new(
                address.city.as_deref(),
                address.country.as_deref(),
                address.line1.as_deref(),
                address.line2.as_deref(),
                address.postal_code.as_deref(),
                address.state.as_deref(),
            );
            let address = new_address.create(&mut write).await?;
            let update_org = UpdateOrg {
                id: org.id,
                name: None,
                address_id: Some(address.id),
            };
            update_org.update(&mut write).await?;
        }
        Some(Err(err)) => return Err(err.into()),
    };
    Ok(api::OrgServiceSetAddressResponse {})
}

async fn delete_address(
    req: api::OrgServiceDeleteAddressRequest,
    meta: MetadataMap,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceDeleteAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write.auth(&meta, OrgAddressPerm::Delete, org_id).await?;
    let org = Org::by_id(org_id, &mut write).await?;
    let customer_id = org
        .stripe_customer_id
        .as_deref()
        .ok_or(Error::NoStripeCustomer(org_id))?;
    write.ctx.stripe.delete_address(customer_id).await?;
    Ok(api::OrgServiceDeleteAddressResponse {})
}

async fn get_invoices(
    req: api::OrgServiceGetInvoicesRequest,
    meta: MetadataMap,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetInvoicesResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write.auth(&meta, OrgAddressPerm::Delete, org_id).await?;
    let org = Org::by_id(org_id, &mut write).await?;
    let customer_id = org
        .stripe_customer_id
        .as_deref()
        .ok_or(Error::NoStripeCustomer(org_id))?;
    let invoices = write.ctx.stripe.get_invoices(customer_id).await?;
    let invoices = invoices
        .into_iter()
        .map(api::Invoice::from_stripe)
        .collect::<Result<_, _>>()?;
    Ok(api::OrgServiceGetInvoicesResponse { invoices })
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

        let mut org_users = OrgUsers::for_org_ids(&org_ids, conn).await?;
        let mut invitations = Invitation::for_org_ids(&org_ids, conn).await?;

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

                let invitations = invitations
                    .remove(&org.id)
                    .unwrap_or_default()
                    .to_map_keep_last(|inv| (inv.invitee_email.clone(), inv));

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
                            joined_at: invitations
                                .get(&user.email)
                                .and_then(|inv| inv.accepted_at)
                                .map(|time| NanosUtc::from(time).into()),
                        })
                    })
                    .collect();

                Ok(api::Org {
                    id: org.id.to_string(),
                    name: org.name.clone(),
                    personal: org.is_personal,
                    created_at: Some(NanosUtc::from(org.created_at).into()),
                    updated_at: Some(NanosUtc::from(org.updated_at).into()),
                    host_count: u64::try_from(max(0, org.host_count)).map_err(Error::ParseMax)?,
                    node_count: u64::try_from(max(0, org.node_count)).map_err(Error::ParseMax)?,
                    member_count: u64::try_from(max(0, org.member_count))
                        .map_err(Error::ParseMax)?,
                    members,
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
                    api::OrgSortField::HostCount => Ok(OrgSort::HostCount(order)),
                    api::OrgSortField::NodeCount => Ok(OrgSort::NodeCount(order)),
                    api::OrgSortField::MemberCount => Ok(OrgSort::MemberCount(order)),
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

impl api::Invoice {
    fn from_stripe(value: crate::stripe::api::invoice::Invoice) -> Result<Self, Error> {
        Ok(Self {
            number: value.number,
            created_at: value
                .created
                .and_then(|value| chrono::DateTime::from_timestamp(value.0, 0))
                .map(NanosUtc::from)
                .map(Into::into),
            discount: value.discount.map(api::Discount::from_stripe),
            pdf_url: value.invoice_pdf,
            line_items: value
                .lines
                .map(|lines| lines.data)
                .unwrap_or_default()
                .into_iter()
                .map(|item| {
                    Ok(api::LineItem {
                        subtotal: item.amount.try_into().map_err(Error::NegativePrice)?,
                        total: item
                            .price
                            .and_then(|p| p.unit_amount)
                            .map(|amount| amount.try_into().map_err(Error::NegativePrice))
                            .transpose()?,
                        description: item.description,
                        start: item
                            .period
                            .as_ref()
                            .and_then(|p| p.start.as_ref())
                            .and_then(|start| chrono::DateTime::from_timestamp(start.0, 0))
                            .map(NanosUtc::from)
                            .map(Into::into),
                        end: item
                            .period
                            .as_ref()
                            .and_then(|p| p.end.as_ref())
                            .and_then(|end| chrono::DateTime::from_timestamp(end.0, 0))
                            .map(NanosUtc::from)
                            .map(Into::into),
                        plan: item.plan.and_then(|plan| plan.nickname),
                        proration: item.proration,
                    })
                })
                .collect::<Result<_, Error>>()?,
            status: value
                .status
                .map(|status| api::InvoiceStatus::from_stripe(status) as i32),
            subtotal: value
                .subtotal
                .map(|sub| sub.try_into().map_err(Error::NegativePrice))
                .transpose()?,
            total: value
                .total
                .map(|tot| tot.try_into().map_err(Error::NegativePrice))
                .transpose()?,
        })
    }
}

impl api::Discount {
    fn from_stripe(value: crate::stripe::api::discount::Discount) -> Self {
        Self {
            name: value.coupon.name,
        }
    }
}

impl common::Address {
    fn from_stripe(value: crate::stripe::api::address::Address) -> Self {
        Self {
            city: value.city,
            country: value.country,
            line1: value.line1,
            line2: value.line2,
            postal_code: value.postal_code,
            state: value.state,
        }
    }
}

impl api::InvoiceStatus {
    pub const fn from_stripe(value: crate::stripe::api::invoice::InvoiceStatus) -> Self {
        use crate::stripe::api::invoice::InvoiceStatus::*;
        match value {
            Draft => api::InvoiceStatus::Draft,
            Open => api::InvoiceStatus::Open,
            Paid => api::InvoiceStatus::Paid,
            Uncollectible => api::InvoiceStatus::Uncollectible,
            Void => api::InvoiceStatus::Void,
        }
    }
}
