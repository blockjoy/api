use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::{ProtocolAdminPerm, ProtocolPerm};
use crate::auth::resource::Resources;
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::model::protocol::stats::NodeStats;
use crate::model::protocol::version::{
    NewVersion, ProtocolKey, ProtocolVersion, UpdateVersion, VersionKey,
};
use crate::model::protocol::{
    NewProtocol, Protocol, ProtocolFilter, ProtocolSearch, ProtocolSort, UpdateProtocol,
};
use crate::model::{Region, RegionId};
use crate::util::{HashVec, NanosUtc};

use super::api::protocol_service_server::ProtocolService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Protocol command failed: {0}
    Command(#[from] crate::model::command::Error),
    /// Protocol grpc command failed: {0}
    CommandGrpc(#[from] crate::grpc::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing `api::Protocol` model output. This should not happen.
    MissingModel,
    /// Missing protocol.
    MissingProtocol,
    /// Missing StatsFor.
    MissingStatsFor,
    /// Missing version key.
    MissingVersionKey,
    /// Protocol node error: {0}
    Node(#[from] crate::model::node::Error),
    /// Protocol node log error: {0}
    NodeLog(#[from] crate::model::node::log::Error),
    /// Failed to parse ProtocolId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse protocol version: {0}
    ParseVersion(crate::util::sql::Error),
    /// Failed to parse VersionId: {0}
    ParseVersionId(uuid::Error),
    /// Protocol model error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Protocol stats error: {0}
    ProtocolStats(#[from] crate::model::protocol::stats::Error),
    /// Protocol version error: {0}
    ProtocolVersion(#[from] crate::model::protocol::version::Error),
    /// Region error: {0}
    Region(#[from] crate::model::region::Error),
    /// The region `{0}` has no pricing set.
    RegionMissingPrice(RegionId),
    /// Protocol search failed: {0}
    SearchOperator(crate::util::search::Error),
    /// Sort order: {0}
    SortOrder(crate::util::search::Error),
    /// Store failed: {0}
    Store(#[from] crate::store::Error),
    /// Stripe error: {0}
    Stripe(#[from] crate::stripe::Error),
    /// Stripe Price errror: {0}
    StripePrice(#[from] crate::stripe::api::price::Error),
    /// The requested sort field is unknown.
    UnknownSortField,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | MissingModel | Store(_) | Stripe(_) | StripePrice(_) => {
                Status::internal("Internal error.")
            }
            MissingProtocol => Status::invalid_argument("protocol"),
            MissingStatsFor => Status::invalid_argument("stats_for"),
            MissingVersionKey => Status::invalid_argument("version_key"),
            ParseId(_) => Status::invalid_argument("id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseVersion(_) => Status::invalid_argument("protocol_version"),
            ParseVersionId(_) => Status::invalid_argument("protocol_version_id"),
            RegionMissingPrice(_) => Status::not_found("Not found."),
            SearchOperator(_) => Status::invalid_argument("search.operator"),
            SortOrder(_) => Status::invalid_argument("sort.order"),
            UnknownSortField => Status::invalid_argument("sort.field"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            CommandGrpc(err) => err.into(),
            Node(err) => err.into(),
            NodeLog(err) => err.into(),
            Protocol(err) => err.into(),
            ProtocolStats(err) => err.into(),
            ProtocolVersion(err) => err.into(),
            Region(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ProtocolService for Grpc {
    async fn add_protocol(
        &self,
        req: Request<api::ProtocolServiceAddProtocolRequest>,
    ) -> Result<Response<api::ProtocolServiceAddProtocolResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| add_protocol(req, meta, write).scope_boxed())
            .await
    }

    async fn add_version(
        &self,
        req: Request<api::ProtocolServiceAddVersionRequest>,
    ) -> Result<Response<api::ProtocolServiceAddVersionResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| add_version(req, meta, write).scope_boxed())
            .await
    }

    async fn get_latest(
        &self,
        req: Request<api::ProtocolServiceGetLatestRequest>,
    ) -> Result<Response<api::ProtocolServiceGetLatestResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_latest(req, meta, read).scope_boxed())
            .await
    }

    async fn get_pricing(
        &self,
        req: Request<api::ProtocolServiceGetPricingRequest>,
    ) -> Result<Response<api::ProtocolServiceGetPricingResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_pricing(req, meta, read).scope_boxed())
            .await
    }

    async fn get_protocol(
        &self,
        req: Request<api::ProtocolServiceGetProtocolRequest>,
    ) -> Result<Response<api::ProtocolServiceGetProtocolResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_protocol(req, meta, read).scope_boxed())
            .await
    }

    async fn get_stats(
        &self,
        req: Request<api::ProtocolServiceGetStatsRequest>,
    ) -> Result<Response<api::ProtocolServiceGetStatsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_stats(req, meta, read).scope_boxed())
            .await
    }

    async fn list_protocols(
        &self,
        req: Request<api::ProtocolServiceListProtocolsRequest>,
    ) -> Result<Response<api::ProtocolServiceListProtocolsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_protocols(req, meta, read).scope_boxed())
            .await
    }

    async fn list_versions(
        &self,
        req: Request<api::ProtocolServiceListVersionsRequest>,
    ) -> Result<Response<api::ProtocolServiceListVersionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_versions(req, meta, read).scope_boxed())
            .await
    }

    async fn update_protocol(
        &self,
        req: Request<api::ProtocolServiceUpdateProtocolRequest>,
    ) -> Result<Response<api::ProtocolServiceUpdateProtocolResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_protocol(req, meta, write).scope_boxed())
            .await
    }

    async fn update_version(
        &self,
        req: Request<api::ProtocolServiceUpdateVersionRequest>,
    ) -> Result<Response<api::ProtocolServiceUpdateVersionResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_version(req, meta, write).scope_boxed())
            .await
    }
}

async fn add_protocol(
    req: api::ProtocolServiceAddProtocolRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ProtocolServiceAddProtocolResponse, Error> {
    let authz = write.auth(&meta, ProtocolAdminPerm::AddProtocol).await?;

    let org_id = req
        .org_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let new_protocol = NewProtocol {
        org_id,
        key: ProtocolKey::new(req.key)?,
        name: req.name,
        description: req.description,
        ticker: req.ticker,
    };

    let protocol = new_protocol.create(&mut write).await?;
    let protocol = api::Protocol::from_model(protocol, &authz, &mut write).await?;

    Ok(api::ProtocolServiceAddProtocolResponse {
        protocol: Some(protocol),
    })
}

async fn add_version(
    req: api::ProtocolServiceAddVersionRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ProtocolServiceAddVersionResponse, Error> {
    let authz = write.auth(&meta, ProtocolAdminPerm::AddVersion).await?;

    let org_id = req
        .org_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let version_key = VersionKey::try_from(req.version_key.ok_or(Error::MissingVersionKey)?)?;
    let protocol = Protocol::by_key(&version_key.protocol_key, org_id, &authz, &mut write).await?;

    let new_version = NewVersion {
        org_id: protocol.org_id.or(org_id),
        protocol_id: protocol.id,
        protocol_key: version_key.protocol_key,
        variant_key: version_key.variant_key,
        semantic_version: &req.semantic_version.parse().map_err(Error::ParseVersion)?,
        sku_code: &req.sku_code,
        description: req.description,
    };
    let version = new_version.create(&mut write).await?;

    Ok(api::ProtocolServiceAddVersionResponse {
        version: Some(version.into()),
    })
}

async fn get_latest(
    req: api::ProtocolServiceGetLatestRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceGetLatestResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(
            &meta,
            ProtocolAdminPerm::GetLatest,
            ProtocolPerm::GetLatest,
            resources,
        )
        .await?;

    let version_key = VersionKey::try_from(req.version_key.ok_or(Error::MissingVersionKey)?)?;
    let version = ProtocolVersion::latest_by_key(&version_key, org_id, &authz, &mut read).await?;

    Ok(api::ProtocolServiceGetLatestResponse {
        protocol_version: Some(version.into()),
    })
}

async fn get_pricing(
    req: api::ProtocolServiceGetPricingRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceGetPricingResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_for(&meta, ProtocolPerm::GetPricing, resources)
        .await?;

    let version_key = VersionKey::try_from(req.version_key.ok_or(Error::MissingVersionKey)?)?;
    let version = ProtocolVersion::latest_by_key(&version_key, org_id, &authz, &mut read).await?;

    let region = Region::by_name(&req.region, &mut read).await?;
    let sku = version
        .sku(&region)
        .ok_or(Error::RegionMissingPrice(region.id))?;
    let price = read.ctx.stripe.get_price(&sku).await?;

    Ok(api::ProtocolServiceGetPricingResponse {
        billing_amount: Some(common::BillingAmount::try_from(&price)?),
    })
}

async fn get_protocol(
    req: api::ProtocolServiceGetProtocolRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceGetProtocolResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(
            &meta,
            ProtocolAdminPerm::GetProtocol,
            ProtocolPerm::GetProtocol,
            resources,
        )
        .await?;

    let protocol = match req.protocol.ok_or(Error::MissingProtocol)? {
        api::protocol_service_get_protocol_request::Protocol::ProtocolId(id) => {
            let id = id.parse().map_err(Error::ParseId)?;
            Protocol::by_id(id, org_id, &authz, &mut read).await?
        }
        api::protocol_service_get_protocol_request::Protocol::ProtocolKey(key) => {
            let key = ProtocolKey::new(key)?;
            Protocol::by_key(&key, org_id, &authz, &mut read).await?
        }
    };

    Ok(api::ProtocolServiceGetProtocolResponse {
        protocol: Some(api::Protocol::from_model(protocol, &authz, &mut read).await?),
    })
}

async fn get_stats(
    req: api::ProtocolServiceGetStatsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceGetStatsResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(
            &meta,
            ProtocolAdminPerm::ViewAllStats,
            ProtocolPerm::GetStats,
            resources,
        )
        .await?;

    match req.stats_for.ok_or(Error::MissingStatsFor)? {
        api::protocol_service_get_stats_request::StatsFor::ProtocolId(id) => {
            let id = id.parse().map_err(Error::ParseId)?;
            let protocol = Protocol::by_id(id, org_id, &authz, &mut read).await?;
            let stats = NodeStats::for_protocol(&protocol, &mut read).await?;

            Ok(api::ProtocolServiceGetStatsResponse {
                protocol_stats: hashmap! { id.to_string() => stats.try_into()? },
                version_stats: hashmap! {},
            })
        }
        api::protocol_service_get_stats_request::StatsFor::ProtocolVersionId(id) => {
            let id = id.parse().map_err(Error::ParseVersionId)?;
            let version = ProtocolVersion::by_id(id, org_id, &authz, &mut read).await?;
            let stats = NodeStats::for_version(&version, &mut read).await?;

            Ok(api::ProtocolServiceGetStatsResponse {
                protocol_stats: hashmap! {},
                version_stats: hashmap! { id.to_string() => stats.try_into()? },
            })
        }
        api::protocol_service_get_stats_request::StatsFor::AllProtocols(()) => {
            let stats = NodeStats::for_all_protocols(&authz, &mut read).await?;
            let protocol_stats = stats
                .into_iter()
                .map(|(protocol_id, stats)| Ok((protocol_id.to_string(), stats.try_into()?)))
                .collect::<Result<_, Error>>()?;

            Ok(api::ProtocolServiceGetStatsResponse {
                protocol_stats,
                version_stats: hashmap! {},
            })
        }
        api::protocol_service_get_stats_request::StatsFor::AllVersions(()) => {
            let stats = NodeStats::for_all_versions(&authz, &mut read).await?;
            let version_stats = stats
                .into_iter()
                .map(|(version_id, stats)| Ok((version_id.to_string(), stats.try_into()?)))
                .collect::<Result<_, Error>>()?;

            Ok(api::ProtocolServiceGetStatsResponse {
                protocol_stats: hashmap! {},
                version_stats,
            })
        }
    }
}

async fn list_protocols(
    req: api::ProtocolServiceListProtocolsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceListProtocolsResponse, Error> {
    let org_ids = req
        .org_ids
        .iter()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .collect::<Result<HashSet<_>, _>>()?;
    let authz = read
        .auth_or_for(
            &meta,
            ProtocolAdminPerm::ListProtocols,
            ProtocolPerm::ListProtocols,
            &org_ids,
        )
        .await?;

    let search = req
        .search
        .map(|search| {
            Ok::<_, Error>(ProtocolSearch {
                operator: search
                    .operator()
                    .try_into()
                    .map_err(Error::SearchOperator)?,
                id: search.protocol_id.map(|name| name.trim().to_lowercase()),
                name: search.name.map(|name| name.trim().to_lowercase()),
            })
        })
        .transpose()?;
    let sort = req
        .sort
        .into_iter()
        .map(|sort| {
            let order = sort.order().try_into().map_err(Error::SortOrder)?;
            match sort.field() {
                api::ProtocolSortField::Unspecified => Err(Error::UnknownSortField),
                api::ProtocolSortField::Key => Ok(ProtocolSort::Key(order)),
                api::ProtocolSortField::Name => Ok(ProtocolSort::Name(order)),
            }
        })
        .collect::<Result<_, _>>()?;

    let filter = ProtocolFilter {
        org_ids: org_ids.clone(),
        offset: req.offset,
        limit: req.limit,
        search,
        sort,
    };

    let (protocols, _) = filter.query(&authz, &mut read).await?;
    let mut protocols = api::Protocol::from_models(protocols, &authz, &mut read).await?;
    protocols.sort_by_cached_key(|chain| chain.name.clone());

    Ok(api::ProtocolServiceListProtocolsResponse { protocols })
}

async fn list_versions(
    req: api::ProtocolServiceListVersionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ProtocolServiceListVersionsResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(
            &meta,
            ProtocolAdminPerm::ListVersions,
            ProtocolPerm::ListVersions,
            resources,
        )
        .await?;

    let version_key = VersionKey::try_from(req.version_key.ok_or(Error::MissingVersionKey)?)?;
    let versions = ProtocolVersion::by_key(&version_key, org_id, &authz, &mut read).await?;

    Ok(api::ProtocolServiceListVersionsResponse {
        protocol_versions: versions.into_iter().map(Into::into).collect(),
    })
}

async fn update_protocol(
    req: api::ProtocolServiceUpdateProtocolRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ProtocolServiceUpdateProtocolResponse, Error> {
    let authz = write.auth(&meta, ProtocolAdminPerm::UpdateProtocol).await?;

    let update = UpdateProtocol {
        id: req.protocol_id.parse().map_err(Error::ParseId)?,
        name: req.name.as_deref(),
        description: req.description.as_deref(),
        visibility: req
            .visibility
            .map(|_| req.visibility().try_into())
            .transpose()?,
    };

    let protocol = update.apply(&mut write).await?;
    let protocol = api::Protocol::from_model(protocol, &authz, &mut write).await?;

    Ok(api::ProtocolServiceUpdateProtocolResponse {
        protocol: Some(protocol),
    })
}

async fn update_version(
    req: api::ProtocolServiceUpdateVersionRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ProtocolServiceUpdateVersionResponse, Error> {
    let _authz = write.auth(&meta, ProtocolAdminPerm::UpdateVersion).await?;

    let id = req
        .protocol_version_id
        .parse()
        .map_err(Error::ParseVersionId)?;

    let update = UpdateVersion {
        id,
        sku_code: req.sku_code.as_deref(),
        description: req.description.as_deref(),
        visibility: req
            .visibility
            .map(|_| req.visibility().try_into())
            .transpose()?,
    };
    let version = update.apply(&mut write).await?;

    Ok(api::ProtocolServiceUpdateVersionResponse {
        protocol_version: Some(version.into()),
    })
}

impl api::Protocol {
    async fn from_models(
        protocols: Vec<Protocol>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let ids: HashSet<_> = protocols.iter().map(|protocol| protocol.id).collect();
        let org_ids: HashSet<_> = protocols
            .iter()
            .filter_map(|protocol| protocol.org_id)
            .collect();

        let versions = ProtocolVersion::by_protocol_ids(&ids, &org_ids, authz, conn).await?;
        let mut versions = versions.to_map_keep_all(|v| (v.protocol_id, v));

        protocols
            .into_iter()
            .map(|protocol| {
                Ok(api::Protocol {
                    protocol_id: protocol.id.to_string(),
                    org_id: protocol.org_id.map(|id| id.to_string()),
                    key: protocol.key.into(),
                    name: protocol.name,
                    description: protocol.description,
                    ticker: protocol.ticker,
                    visibility: common::Visibility::from(protocol.visibility).into(),
                    created_at: Some(NanosUtc::from(protocol.created_at).into()),
                    updated_at: protocol.updated_at.map(|at| NanosUtc::from(at).into()),
                    versions: versions
                        .remove(&protocol.id)
                        .map(|versions| versions.into_iter().map(Into::into).collect())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn from_model(
        protocol: Protocol,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let mut protocols = Self::from_models(vec![protocol], authz, conn)
            .await?
            .into_iter();

        match (protocols.next(), protocols.next()) {
            (Some(protocol), None) => Ok(protocol),
            _ => Err(Error::MissingModel),
        }
    }
}
