use std::collections::HashSet;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::{ImageAdminPerm, ImagePerm};
use crate::auth::resource::Resources;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::model::image::archive::{NewArchive, UpdateArchive};
use crate::model::image::config::Ramdisks;
use crate::model::image::property::ImagePropertyKey;
use crate::model::image::rule::{ImageRule, NewImageRule};
use crate::model::image::{Archive, Image, ImageProperty, NewImage, NewProperty, UpdateImage};
use crate::model::protocol::VersionKey;
use crate::model::ProtocolVersion;
use crate::store::StoreId;
use crate::util::sql::Version;
use crate::util::{HashVec, NanosUtc};

use super::api::image_service_server::ImageService;
use super::{api, common, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Image archive error: {0}
    Archive(#[from] crate::model::image::archive::Error),
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Failed to parse build_version: {0}
    BuildVersion(std::num::TryFromIntError),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Failed to find semantic version: {0}
    FindVersion(Version),
    /// Image model error: {0}
    Image(#[from] crate::model::image::Error),
    /// Invalid new_archive_keys combination: {0:?}
    InvalidKeyCombo(HashSet<ImagePropertyKey>),
    /// Failed to parse minimum cpu count: {0}
    MinCpu(std::num::TryFromIntError),
    /// Failed to parse minimum disk space: {0}
    MinDisk(std::num::TryFromIntError),
    /// Failed to parse minimum memory: {0}
    MinMemory(std::num::TryFromIntError),
    /// Missing firewall config.
    MissingFirewallConfig,
    /// Missing expected new_archive key combos: {0:?}
    MissingKeyCombos(Vec<HashSet<ImagePropertyKey>>),
    /// Missing image property key: {0}
    MissingPropertyKey(ImagePropertyKey),
    /// Missing StoreId pointer.
    MissingStorePointer,
    /// Missing version key.
    MissingVersionKey,
    /// No builds found.
    NoBuilds,
    /// No versions found.
    NoVersions,
    /// Failed to parse ArchiveId: {0}
    ParseArchiveId(uuid::Error),
    /// Failed to parse ImageId: {0}
    ParseImageId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Failed to parse ProtocolId: {0}
    ParseProtocolId(uuid::Error),
    /// Failed to parse protocol version: {0}
    ParseVersion(crate::util::sql::Error),
    /// Failed to parse VersionId: {0}
    ParseVersionId(uuid::Error),
    /// Image property error: {0}
    Property(#[from] crate::model::image::property::Error),
    /// Image protocol error: {0}
    Protocol(#[from] crate::model::protocol::Error),
    /// Image firewall rule error: {0}
    Rule(#[from] crate::model::image::rule::Error),
    /// Image store error: {0}
    Store(#[from] crate::store::Error),
    /// Image protocol version error: {0}
    Version(#[from] crate::model::protocol::version::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) => Status::internal("Internal error."),
            FindVersion(_) | NoBuilds | NoVersions => Status::not_found("Not found."),
            BuildVersion(_) => Status::invalid_argument("build_version"),
            InvalidKeyCombo(set) => {
                // safety: keys are from the client
                Status::invalid_argument(format!("invalid archive_pointer key combo: {set:?}"))
            }
            MinCpu(_) => Status::invalid_argument("min_cpu_cores"),
            MinDisk(_) => Status::invalid_argument("min_disk_bytes"),
            MinMemory(_) => Status::invalid_argument("min_memory_bytes"),
            MissingFirewallConfig => Status::invalid_argument("firewall"),
            MissingKeyCombos(set) => {
                // safety: keys are from properties the client provided
                Status::invalid_argument(format!("missing archive_pointer key combos: {set:?}"))
            }
            MissingPropertyKey(key) => {
                // safety: keys is from the client
                Status::invalid_argument(format!("missing archive_pointer key: {key}"))
            }
            MissingStorePointer => Status::invalid_argument("archive_pointer.pointer"),
            MissingVersionKey => Status::invalid_argument("version_key"),
            ParseArchiveId(_) => Status::invalid_argument("id"),
            ParseImageId(_) => Status::invalid_argument("image_id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            ParseProtocolId(_) => Status::invalid_argument("protocol_id"),
            ParseVersion(_) => Status::invalid_argument("protocol_version"),
            ParseVersionId(_) => Status::invalid_argument("protocol_version_id"),
            Archive(err) => err.into(),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Image(err) => err.into(),
            Property(err) => err.into(),
            Protocol(err) => err.into(),
            Rule(err) => err.into(),
            Store(err) => err.into(),
            Version(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl ImageService for Grpc {
    async fn add_image(
        &self,
        req: Request<api::ImageServiceAddImageRequest>,
    ) -> Result<Response<api::ImageServiceAddImageResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| add_image(req, meta, write).scope_boxed())
            .await
    }

    async fn get_image(
        &self,
        req: Request<api::ImageServiceGetImageRequest>,
    ) -> Result<Response<api::ImageServiceGetImageResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| get_image(req, meta, read).scope_boxed())
            .await
    }

    async fn list_archives(
        &self,
        req: Request<api::ImageServiceListArchivesRequest>,
    ) -> Result<Response<api::ImageServiceListArchivesResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_archives(req, meta, read).scope_boxed())
            .await
    }

    async fn update_archive(
        &self,
        req: Request<api::ImageServiceUpdateArchiveRequest>,
    ) -> Result<Response<api::ImageServiceUpdateArchiveResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_archive(req, meta, write).scope_boxed())
            .await
    }

    async fn update_image(
        &self,
        req: Request<api::ImageServiceUpdateImageRequest>,
    ) -> Result<Response<api::ImageServiceUpdateImageResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update_image(req, meta, write).scope_boxed())
            .await
    }
}

async fn add_image(
    req: api::ImageServiceAddImageRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ImageServiceAddImageResponse, Error> {
    let authz = write.auth(&meta, ImageAdminPerm::Add).await?;

    let version_id = req
        .protocol_version_id
        .parse()
        .map_err(Error::ParseVersionId)?;
    let org_id = req
        .org_id
        .as_ref()
        .map(|id| id.parse().map_err(Error::ParseOrgId))
        .transpose()?;

    let version = ProtocolVersion::by_id(version_id, org_id, &authz, &mut write).await?;
    let latest = Image::latest_build(version_id, &mut write).await?;

    let firewall = req.firewall.ok_or(Error::MissingFirewallConfig)?;
    let new_image = NewImage {
        protocol_version_id: version.id,
        org_id: version.org_id.or(org_id),
        image_uri: req.image_uri,
        build_version: latest.map_or(1, |build| build + 1),
        description: req.description,
        min_cpu_cores: i64::try_from(req.min_cpu_cores).map_err(Error::MinCpu)?,
        min_memory_bytes: i64::try_from(req.min_memory_bytes).map_err(Error::MinMemory)?,
        min_disk_bytes: i64::try_from(req.min_disk_bytes).map_err(Error::MinDisk)?,
        ramdisks: Ramdisks(req.ramdisks.into_iter().map(Into::into).collect()),
        default_firewall_in: firewall.default_in().try_into()?,
        default_firewall_out: firewall.default_out().try_into()?,
    };
    let image = new_image.create(&mut write).await?;

    let new_rules = firewall
        .rules
        .into_iter()
        .map(|rule| NewImageRule::from_api(image.id, rule))
        .collect::<Result<_, _>>()?;
    let rules = NewImageRule::bulk_create(new_rules, &mut write).await?;

    let new_properties = req
        .properties
        .into_iter()
        .map(|prop| NewProperty::from(image.id, prop))
        .collect::<Result<_, _>>()?;
    let properties = NewProperty::bulk_create(new_properties, &mut write).await?;
    let key_to_property_id = properties
        .iter()
        .to_map_keep_last(|prop| (prop.key.clone(), prop.id));

    // find all possible new_archive key combinations
    let mut new_archive_powerset = properties
        .iter()
        .filter_map(|prop| prop.new_archive.then_some(prop.key.clone()))
        .fold(vec![hashset! {}], |sets, key| {
            sets.iter()
                .cloned()
                .chain(sets.iter().map(|set| {
                    let mut new_set = set.clone();
                    new_set.insert(key.clone());
                    new_set
                }))
                .collect()
        });

    // get the store pointers for all requested new_archive combinations
    let archive_pointers = req
        .archive_pointers
        .into_iter()
        .map(|pointer| {
            let keys = pointer
                .new_archive_keys
                .into_iter()
                .map(ImagePropertyKey)
                .collect::<HashSet<_>>();
            let ids = keys
                .iter()
                .map(|key| {
                    key_to_property_id
                        .get(key)
                        .copied()
                        .ok_or_else(|| Error::MissingPropertyKey(key.clone()))
                })
                .collect::<Result<HashSet<_>, _>>()?;
            let pointer = match pointer.pointer.ok_or(Error::MissingStorePointer)? {
                api::archive_pointer::Pointer::StoreId(id) => Some(StoreId::new(id)?),
                api::archive_pointer::Pointer::Disallowed(()) => None,
            };
            Ok((keys, ids, pointer))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    // create new archives for all valid new_archive key combinations
    let mut new_archives = Vec::with_capacity(archive_pointers.len());
    for (keys, ids, pointer) in archive_pointers {
        let mut found = None;
        for (index, set) in new_archive_powerset.iter().enumerate() {
            if keys == *set {
                found = Some(index);
                if let Some(ref store_id) = pointer {
                    new_archives.push(NewArchive::new(image.id, store_id.clone(), &ids));
                }
            }
        }

        if let Some(index) = found {
            let _ = new_archive_powerset.swap_remove(index);
        } else {
            return Err(Error::InvalidKeyCombo(keys));
        }
    }

    // ensure all possible new_archive key combinations are provided
    let archives = if new_archive_powerset.is_empty() {
        NewArchive::bulk_create(new_archives, &mut write).await?
    } else {
        return Err(Error::MissingKeyCombos(new_archive_powerset));
    };

    Ok(api::ImageServiceAddImageResponse {
        image: Some(api::Image::from(image, properties, rules)?),
        archives: archives.into_iter().map(Into::into).collect(),
    })
}

async fn get_image(
    req: api::ImageServiceGetImageRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ImageServiceGetImageResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(&meta, ImageAdminPerm::Get, ImagePerm::Get, resources)
        .await?;

    let version_key = VersionKey::try_from(req.version_key.ok_or(Error::MissingVersionKey)?)?;
    let mut versions = ProtocolVersion::by_key(&version_key, org_id, &authz, &mut read).await?;

    let version = if let Some(version) = req.semantic_version {
        let version: Version = version.parse().map_err(Error::ParseVersion)?;
        versions
            .into_iter()
            .find(|v| v.semantic_version == version)
            .ok_or(Error::FindVersion(version))?
    } else {
        versions.pop().ok_or(Error::NoVersions)?
    };

    let build = if let Some(build) = req.build_version {
        i64::try_from(build).map_err(Error::BuildVersion)?
    } else {
        Image::latest_build(version.id, &mut read)
            .await?
            .ok_or(Error::NoBuilds)?
    };

    let image = Image::by_build(version.id, org_id, build, &authz, &mut read).await?;
    let properties = ImageProperty::by_image_id(image.id, &mut read).await?;
    let rules = ImageRule::by_image_id(image.id, &mut read).await?;

    Ok(api::ImageServiceGetImageResponse {
        image: Some(api::Image::from(image, properties, rules)?),
    })
}

async fn list_archives(
    req: api::ImageServiceListArchivesRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::ImageServiceListArchivesResponse, Error> {
    let (org_id, resources): (_, Resources) = if let Some(ref org_id) = req.org_id {
        let org_id = org_id.parse().map_err(Error::ParseOrgId)?;
        (Some(org_id), [org_id].into())
    } else {
        (None, Resources::None)
    };

    let authz = read
        .auth_or_for(
            &meta,
            ImageAdminPerm::ListArchives,
            ImagePerm::ListArchives,
            resources,
        )
        .await?;

    let image_id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let image = Image::by_id(image_id, org_id, &authz, &mut read).await?;
    let archives = Archive::by_image_id(image.id, org_id, &mut read).await?;

    Ok(api::ImageServiceListArchivesResponse {
        archives: archives.into_iter().map(Into::into).collect(),
    })
}

async fn update_archive(
    req: api::ImageServiceUpdateArchiveRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ImageServiceUpdateArchiveResponse, Error> {
    let _authz = write.auth(&meta, ImageAdminPerm::UpdateArchive).await?;

    let id = req.archive_id.parse().map_err(Error::ParseArchiveId)?;
    let store_id = req.store_id.map(Into::into);

    let update = UpdateArchive { id, store_id };
    let archive = update.update(&mut write).await?;

    Ok(api::ImageServiceUpdateArchiveResponse {
        archive: Some(archive.into()),
    })
}

async fn update_image(
    req: api::ImageServiceUpdateImageRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::ImageServiceUpdateImageResponse, Error> {
    let _authz = write.auth(&meta, ImageAdminPerm::UpdateImage).await?;

    let id = req.image_id.parse().map_err(Error::ParseImageId)?;
    let visibility = req
        .visibility
        .map(|_| req.visibility().try_into())
        .transpose()?;

    let update = UpdateImage { id, visibility };
    let image = update.update(&mut write).await?;

    let properties = ImageProperty::by_image_id(image.id, &mut write).await?;
    let rules = ImageRule::by_image_id(image.id, &mut write).await?;

    Ok(api::ImageServiceUpdateImageResponse {
        image: Some(api::Image::from(image, properties, rules)?),
    })
}

impl api::Image {
    pub fn from(
        image: Image,
        properties: Vec<ImageProperty>,
        rules: Vec<ImageRule>,
    ) -> Result<Self, Error> {
        Ok(api::Image {
            image_id: image.id.to_string(),
            protocol_version_id: image.protocol_version_id.to_string(),
            org_id: image.org_id.map(|id| id.to_string()),
            image_uri: image.image_uri,
            build_version: u64::try_from(image.build_version).map_err(Error::BuildVersion)?,
            description: image.description,
            properties: properties.into_iter().map(Into::into).collect(),
            firewall: Some(common::FirewallConfig {
                default_in: common::FirewallAction::from(image.default_firewall_in).into(),
                default_out: common::FirewallAction::from(image.default_firewall_out).into(),
                rules: rules.into_iter().map(Into::into).collect(),
            }),
            min_cpu_cores: u64::try_from(image.min_cpu_cores).map_err(Error::MinCpu)?,
            min_memory_bytes: u64::try_from(image.min_memory_bytes).map_err(Error::MinMemory)?,
            min_disk_bytes: u64::try_from(image.min_disk_bytes).map_err(Error::MinDisk)?,
            ramdisks: image.ramdisks.into_iter().map(Into::into).collect(),
            visibility: common::Visibility::from(image.visibility).into(),
            created_at: Some(NanosUtc::from(image.created_at).into()),
            updated_at: image.updated_at.map(NanosUtc::from).map(Into::into),
        })
    }
}
