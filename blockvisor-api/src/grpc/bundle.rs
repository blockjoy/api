use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{error, warn};

use crate::auth::rbac::BundlePerm;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::bundle_service_server::BundleService;
use crate::grpc::{api, Grpc};
use crate::store::BUNDLE_FILE;
use crate::util::sql::Version;

use super::{Metadata, Status};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing bundle identifier.
    MissingId,
    /// Failed to parse version from key `{0}`: {1}
    ParseVersion(String, crate::util::sql::Error),
    /// Store failed: {0}
    Store(#[from] crate::store::Error),
    /// File name should end in `/{BUNDLE_FILE:?}` but is `{0}`.
    Suffix(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            Diesel(_) | ParseVersion(_, _) | Store(_) | Suffix(_) => {
                Status::internal("Internal error.")
            }
            MissingId => Status::invalid_argument("bundle_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl BundleService for Grpc {
    async fn retrieve(
        &self,
        req: Request<api::BundleServiceRetrieveRequest>,
    ) -> Result<Response<api::BundleServiceRetrieveResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn list_versions(
        &self,
        req: Request<api::BundleServiceListVersionsRequest>,
    ) -> Result<Response<api::BundleServiceListVersionsResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_versions(req, meta.into(), read).scope_boxed())
            .await
    }
}

async fn retrieve(
    req: api::BundleServiceRetrieveRequest,
    meta: Metadata,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceRetrieveResponse, Error> {
    read.auth(&meta, BundlePerm::Retrieve).await?;

    let id = req.bundle_id.ok_or(Error::MissingId)?;
    let url = read.ctx.store.download_bundle(&id.version).await?;

    Ok(api::BundleServiceRetrieveResponse {
        url: url.to_string(),
    })
}

async fn list_versions(
    _: api::BundleServiceListVersionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::BundleServiceListVersionsResponse, Error> {
    read.auth(&meta, BundlePerm::ListVersions).await?;
    let bundle_ids = read.ctx.store.list_bundles().await?;

    Ok(api::BundleServiceListVersionsResponse { bundle_ids })
}

impl api::BundleIdentifier {
    /// Extract the bundle version from a key.
    ///
    /// Example key format: `0.1.0/bvd-bundle.tgz`
    pub fn from_key<K: AsRef<str>>(key: K) -> Result<Self, Error> {
        let key = key.as_ref();
        let version: Version = key
            .strip_suffix(&format!("/{BUNDLE_FILE}"))
            .ok_or_else(|| Error::Suffix(key.into()))?
            .parse()
            .map_err(|err| Error::ParseVersion(key.into(), err))?;

        Ok(api::BundleIdentifier {
            version: version.to_string(),
        })
    }

    /// Try and parse a `BundleIdentifier` from a key, or return None otherwise.
    pub fn maybe_from_key<K: AsRef<str>>(key: K) -> Option<Self> {
        let key = key.as_ref();
        Self::from_key(key)
            .map_err(|err| warn!("Failed to parse bundle key `{key}`: {err}"))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundles_from_key() {
        let tests = [
            ("/bvd-bundle.tgz", false),
            ("0.0.0/tester.txt", false),
            ("0.1.0/bvd-bundle.tgz", true),
            ("0.10.0/bvd-bundle.tgz", true),
        ];

        for (test, pass) in tests {
            let result = api::BundleIdentifier::from_key(test);
            if pass {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }
}
