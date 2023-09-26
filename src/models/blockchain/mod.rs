pub mod node_type;
pub use node_type::{BlockchainNodeType, BlockchainNodeTypeId};

pub mod property;
pub use property::{BlockchainProperty, BlockchainPropertyId, BlockchainPropertyUiType};

pub mod version;
pub use version::{BlockchainVersion, BlockchainVersionId};

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::database::Conn;

use super::schema::blockchains;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to find all blockchains: {0}
    FindAll(diesel::result::Error),
    /// Failed to find blockchain by name `{0}`: {1}
    FindByName(String, diesel::result::Error),
    /// Failed to find blockchain id `{0:?}`: {1}
    FindId(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain ids `{0:?}`: {1}
    FindIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Blockchain Property model error: {0}
    Property(#[from] property::Error),
    /// Failed to update blockchain id `{0:?}`: {1}
    Update(BlockchainId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindAll(NotFound)
            | FindByName(_, NotFound)
            | FindId(_, NotFound)
            | FindIds(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainId(Uuid);

#[derive(Clone, Debug, Queryable, Identifiable, AsChangeset)]
pub struct Blockchain {
    pub id: BlockchainId,
    pub name: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Blockchain {
    pub async fn find_all(conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        blockchains::table
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await
            .map_err(Error::FindAll)
    }

    pub async fn find_by_id(id: BlockchainId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        blockchains::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindId(id, err))
    }

    pub async fn find_by_ids(
        ids: HashSet<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchains::table
            .filter(blockchains::id.eq_any(ids.iter()))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindIds(ids, err))
    }

    pub async fn find_by_name(blockchain: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        blockchains::table
            .filter(super::lower(blockchains::name).eq(super::lower(blockchain)))
            .first(conn)
            .await
            .map_err(|err| Error::FindByName(blockchain.to_lowercase(), err))
    }

    pub async fn update(&self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let mut updated = self.clone();
        updated.updated_at = Utc::now();

        diesel::update(blockchains::table.find(updated.id))
            .set(updated)
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(self.id, err))
    }
}
