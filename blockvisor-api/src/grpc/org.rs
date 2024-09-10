use std::cmp::max;
use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use futures::future::OptionFuture;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::{OrgAddressPerm, OrgAdminPerm, OrgBillingPerm, OrgPerm, OrgProvisionPerm};
use crate::auth::resource::{OrgId, UserId};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::model::address::NewAddress;
use crate::model::org::{NewOrg, OrgFilter, OrgSearch, OrgSort, UpdateOrg};
use crate::model::rbac::{OrgUsers, RbacUser};
use crate::model::{Address, Invitation, Org, Token, User};
use crate::util::{HashVec, NanosUtc};

use super::api::org_service_server::OrgService;
use super::{api, common, Grpc, Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Address error: {0}
    Address(#[from] crate::model::address::Error),
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
    Invitation(#[from] crate::model::invitation::Error),
    /// The request is missing the `address` fields.
    MissingAddress,
    /// No customer exists in stripe for org `{0}`.
    NoStripeCustomer(OrgId),
    /// No subscription exists in stripe for org `{0}`.
    NoStripeSubscription(OrgId),
    /// Org model error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse `id` as OrgId: {0}
    ParseId(uuid::Error),
    /// Failed to parse non-zero count as u64: {0}
    ParseMax(std::num::TryFromIntError),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse UserId: {0}
    ParseUserId(uuid::Error),
    /// Org rbac error: {0}
    Rbac(#[from] crate::model::rbac::Error),
    /// Org resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Cannot remove last owner from an org.
    RemoveLastOwner,
    /// User to remove is not self.
    RemoveNotSelf,
    /// Org search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Stripe error: {0}
    Stripe(#[from] crate::stripe::Error),
    /// Stripe Currency error: {0}
    StripeCurrency(#[from] crate::stripe::api::currency::Error),
    /// Stripe Invoice error: {0}
    StripeInvoice(#[from] crate::stripe::api::invoice::Error),
    /// Org token error: {0}
    Token(#[from] crate::model::token::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
    /// Org user error: {0}
    User(#[from] crate::model::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser | DeletePersonal | RemoveNotSelf => {
                Status::forbidden("Access denied.")
            }
            ConvertNoOrg | Diesel(_) | ParseMax(_) | Stripe(_) | StripeCurrency(_)
            | StripeInvoice(_) => Status::internal("Internal error."),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseUserId(_) => Status::invalid_argument("user_id"),
            RemoveLastOwner => Status::failed_precondition("Can't remove last org owner."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            MissingAddress => Status::failed_precondition("User has no address."),
            NoStripeCustomer(_) => Status::failed_precondition("No customer for that org."),
            NoStripeSubscription(_) => Status::failed_precondition("No subscription for that org."),
            Address(err) => err.into(),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Invitation(err) => err.into(),
            Org(err) => err.into(),
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
    ) -> Result<Response<api::OrgServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: Request<api::OrgServiceGetRequest>,
    ) -> Result<Response<api::OrgServiceGetResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::OrgServiceListRequest>,
    ) -> Result<Response<api::OrgServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::OrgServiceUpdateRequest>,
    ) -> Result<Response<api::OrgServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: Request<api::OrgServiceDeleteRequest>,
    ) -> Result<Response<api::OrgServiceDeleteResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| delete(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn remove_member(
        &self,
        req: Request<api::OrgServiceRemoveMemberRequest>,
    ) -> Result<Response<api::OrgServiceRemoveMemberResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| remove_member(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn get_provision_token(
        &self,
        req: Request<api::OrgServiceGetProvisionTokenRequest>,
    ) -> Result<Response<api::OrgServiceGetProvisionTokenResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_provision_token(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn reset_provision_token(
        &self,
        req: Request<api::OrgServiceResetProvisionTokenRequest>,
    ) -> Result<Response<api::OrgServiceResetProvisionTokenResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| reset_provision_token(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn init_card(
        &self,
        req: Request<api::OrgServiceInitCardRequest>,
    ) -> Result<Response<api::OrgServiceInitCardResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| init_card(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn list_payment_methods(
        &self,
        req: Request<api::OrgServiceListPaymentMethodsRequest>,
    ) -> Result<Response<api::OrgServiceListPaymentMethodsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_payment_methods(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn billing_details(
        &self,
        req: Request<api::OrgServiceBillingDetailsRequest>,
    ) -> Result<Response<api::OrgServiceBillingDetailsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| billing_details(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn get_address(
        &self,
        req: Request<api::OrgServiceGetAddressRequest>,
    ) -> Result<Response<api::OrgServiceGetAddressResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_address(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn set_address(
        &self,
        req: Request<api::OrgServiceSetAddressRequest>,
    ) -> Result<Response<api::OrgServiceSetAddressResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| set_address(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn delete_address(
        &self,
        req: Request<api::OrgServiceDeleteAddressRequest>,
    ) -> Result<Response<api::OrgServiceDeleteAddressResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| delete_address(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn get_invoices(
        &self,
        req: Request<api::OrgServiceGetInvoicesRequest>,
    ) -> Result<Response<api::OrgServiceGetInvoicesResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_invoices(req, meta.into(), read).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::OrgServiceCreateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceCreateResponse, Error> {
    let authz = write.auth(&meta, OrgPerm::Create).await?;
    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;
    let user = User::by_id(user_id, &mut write).await?;

    let new_org = NewOrg {
        name: &req.name,
        is_personal: false,
    };
    let org = new_org.create(user.id, &mut write).await?;
    let org = api::Org::from_model(&org, &mut write).await?;

    let created_by = common::Resource::from(user.id);
    let msg = api::OrgMessage::created(org.clone(), created_by);
    write.mqtt(msg);

    Ok(api::OrgServiceCreateResponse { org: Some(org) })
}

pub async fn get(
    req: api::OrgServiceGetRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseId)?;
    read.auth_or_for(&meta, OrgAdminPerm::Get, OrgPerm::Get, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut read).await?;
    let org = api::Org::from_model(&org, &mut read).await?;

    Ok(api::OrgServiceGetResponse { org: Some(org) })
}

pub async fn list(
    req: api::OrgServiceListRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceListResponse, Error> {
    let filter = req.into_filter()?;
    if let Some(user_id) = filter.member_id {
        read.auth_for(&meta, OrgPerm::List, user_id).await?
    } else {
        read.auth(&meta, OrgAdminPerm::List).await?
    };

    let (orgs, _) = filter.query(&mut read).await?;
    let orgs = api::Org::from_models(&orgs, &mut read).await?;

    Ok(api::OrgServiceListResponse { orgs })
}

pub async fn update(
    req: api::OrgServiceUpdateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceUpdateResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseId)?;
    let authz = write
        .auth_or_for(&meta, OrgAdminPerm::Update, OrgPerm::Update, org_id)
        .await?;

    let update = UpdateOrg {
        id: org_id,
        name: req.name.as_deref(),
        address_id: None,
    };
    let org = update.update(&mut write).await?;
    let org = api::Org::from_model(&org, &mut write).await?;

    let updated_by = common::Resource::from(&authz);
    let msg = api::OrgMessage::updated(org, updated_by);
    write.mqtt(msg);

    Ok(api::OrgServiceUpdateResponse {})
}

pub async fn delete(
    req: api::OrgServiceDeleteRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceDeleteResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseId)?;
    let authz = write.auth_for(&meta, OrgPerm::Delete, org_id).await?;

    let org = Org::by_id(org_id, &mut write).await?;
    if org.is_personal {
        return Err(Error::DeletePersonal);
    }

    org.delete(&mut write).await?;

    let invitations = Invitation::by_org_id(org.id, &mut write).await?;
    let invitation_ids = invitations.into_iter().map(|i| i.id).collect();
    Invitation::bulk_delete(&invitation_ids, &mut write).await?;

    let deleted_by = common::Resource::from(&authz);
    let msg = api::OrgMessage::deleted(&org, deleted_by);
    write.mqtt(msg);

    Ok(api::OrgServiceDeleteResponse {})
}

pub async fn remove_member(
    req: api::OrgServiceRemoveMemberRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceRemoveMemberResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let user_id = req.user_id.parse().map_err(Error::ParseUserId)?;

    let authz = match write.auth_for(&meta, OrgPerm::RemoveMember, org_id).await {
        Ok(authz) => Ok(authz),
        Err(err) => match write.auth_for(&meta, OrgPerm::RemoveSelf, org_id).await {
            Ok(authz) => match authz.resource().user() {
                Some(id) if id == user_id => Ok(authz),
                _ => Err(Error::RemoveNotSelf),
            },
            _ => Err(err.into()),
        },
    }?;

    let user = User::by_id(user_id, &mut write).await?;
    let org = Org::by_id(org_id, &mut write).await?;
    if org.is_personal {
        return Err(Error::DeletePersonal);
    }

    let owners = RbacUser::org_owners(org_id, &mut write).await?;
    if owners.len() == 1 && owners[0] == user_id {
        return Err(Error::RemoveLastOwner);
    }

    Org::remove_user(user_id, org_id, &mut write).await?;
    // To allow re-invitations, remove the already accepted invite.
    Invitation::remove_by_org_user(&user.email, org_id, &mut write).await?;

    let org = api::Org::from_model(&org, &mut write).await?;
    let updated_by = common::Resource::from(&authz);
    let msg = api::OrgMessage::updated(org, updated_by);
    write.mqtt(msg);

    Ok(api::OrgServiceRemoveMemberResponse {})
}

pub async fn get_provision_token(
    req: api::OrgServiceGetProvisionTokenRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetProvisionTokenResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth_for(&meta, OrgProvisionPerm::GetToken, org_id)
        .await?;

    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let token = Token::host_provision_by_user(user_id, org_id, &mut read).await?;

    Ok(api::OrgServiceGetProvisionTokenResponse {
        token: token.token.take(),
    })
}

pub async fn reset_provision_token(
    req: api::OrgServiceResetProvisionTokenRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceResetProvisionTokenResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write
        .auth_for(&meta, OrgProvisionPerm::ResetToken, org_id)
        .await?;

    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let new_token = Token::reset_host_provision(user_id, org_id, &mut write).await?;

    Ok(api::OrgServiceResetProvisionTokenResponse {
        token: new_token.take(),
    })
}

pub async fn init_card(
    req: api::OrgServiceInitCardRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::OrgServiceInitCardResponse, Error> {
    let user_id: UserId = req.user_id.parse().map_err(Error::ParseUserId)?;
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseUserId)?;
    write
        .auth_for(&meta, OrgBillingPerm::InitCard, org_id)
        .await?;

    let client_secret = write
        .ctx
        .stripe
        .create_setup_intent(org_id, user_id)
        .await?
        .client_secret;

    Ok(api::OrgServiceInitCardResponse { client_secret })
}

pub async fn list_payment_methods(
    req: api::OrgServiceListPaymentMethodsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceListPaymentMethodsResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth_for(&meta, OrgBillingPerm::ListPaymentMethods, org_id)
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

pub async fn billing_details(
    req: api::OrgServiceBillingDetailsRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceBillingDetailsResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth_for(&meta, OrgBillingPerm::GetBillingDetails, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut read).await?;

    let Some(customer_id) = org.stripe_customer_id.as_deref() else {
        return Ok(Default::default());
    };
    let subscription = read
        .ctx
        .stripe
        .get_subscription_by_customer(customer_id)
        .await?
        .ok_or_else(|| Error::NoStripeSubscription(org_id))?;

    Ok(api::OrgServiceBillingDetailsResponse {
        currency: common::Currency::try_from(subscription.currency)? as i32,
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
                quantity: Some(item.quantity),
            })
            .collect(),
    })
}

pub async fn get_address(
    req: api::OrgServiceGetAddressRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    read.auth_for(&meta, OrgAddressPerm::Get, org_id).await?;

    let org = Org::by_id(org_id, &mut read).await?;
    let Some(customer_id) = org.stripe_customer_id.as_ref() else {
        return Ok(Default::default());
    };
    let address = read.ctx.stripe.get_address(customer_id).await?;

    Ok(api::OrgServiceGetAddressResponse {
        address: address.map(Into::into),
    })
}

pub async fn set_address(
    req: api::OrgServiceSetAddressRequest,
    meta: Metadata,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceSetAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write.auth_for(&meta, OrgAddressPerm::Set, org_id).await?;

    let org = Org::by_id(org_id, &mut write).await?;
    let address = req.address.ok_or(Error::MissingAddress)?;
    let (org, customer_id) = if let Some(customer_id) = org.stripe_customer_id.clone() {
        (org, customer_id)
    } else {
        let owner = User::owner(org_id, &mut write).await?;
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
        | Some(Err(crate::model::address::Error::FindById(_, diesel::result::Error::NotFound))) => {
            let new_address = NewAddress {
                city: address.city.as_deref(),
                country: address.country.as_deref(),
                line1: address.line1.as_deref(),
                line2: address.line2.as_deref(),
                postal_code: address.postal_code.as_deref(),
                state: address.state.as_deref(),
            };
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

pub async fn delete_address(
    req: api::OrgServiceDeleteAddressRequest,
    meta: Metadata,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceDeleteAddressResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    write
        .auth_for(&meta, OrgAddressPerm::Delete, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut write).await?;
    let customer_id = org
        .stripe_customer_id
        .as_deref()
        .ok_or(Error::NoStripeCustomer(org_id))?;
    write.ctx.stripe.delete_address(customer_id).await?;

    Ok(api::OrgServiceDeleteAddressResponse {})
}

pub async fn get_invoices(
    req: api::OrgServiceGetInvoicesRequest,
    meta: Metadata,
    mut write: ReadConn<'_, '_>,
) -> Result<api::OrgServiceGetInvoicesResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let _authz = write
        .auth_for(&meta, OrgAddressPerm::Delete, org_id)
        .await?;

    let org = Org::by_id(org_id, &mut write).await?;
    let Some(customer_id) = org.stripe_customer_id.as_deref() else {
        return Ok(Default::default());
    };
    let invoices = write.ctx.stripe.get_invoices(customer_id).await?;
    let invoices = invoices
        .into_iter()
        .map(api::Invoice::try_from)
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
        let users = User::by_ids(&user_ids, conn)
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
                    org_id: org.id.to_string(),
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
                    id: search.org_id.map(|id| id.trim().to_lowercase()),
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
