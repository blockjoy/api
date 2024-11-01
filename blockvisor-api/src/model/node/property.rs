use std::collections::HashSet;

use derive_more::{Deref, Display, From};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::NodeId;
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::blockchain::BlockchainPropertyId;
use crate::model::schema::node_properties;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create node properties: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to get node property for node id {0}: {1}
    ByNodeId(NodeId, diesel::result::Error),
    /// Failed to get node property for node ids `{0:?}`: {1}
    ByNodeIds(HashSet<NodeId>, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ByNodeId(_, NotFound) | ByNodeIds(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From)]
pub struct NodePropertyId(Uuid);

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = node_properties)]
pub struct NodeProperty {
    pub id: NodePropertyId,
    pub node_id: NodeId,
    pub blockchain_property_id: BlockchainPropertyId,
    pub value: String,
}

impl NodeProperty {
    pub async fn bulk_create(
        properties: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        diesel::insert_into(node_properties::table)
            .values(properties)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }

    pub async fn by_node_id(node_id: NodeId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_properties::table
            .filter(node_properties::node_id.eq(node_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByNodeId(node_id, err))
    }

    pub async fn by_node_ids(
        node_ids: &HashSet<NodeId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        node_properties::table
            .filter(node_properties::node_id.eq_any(node_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByNodeIds(node_ids.clone(), err))
    }
}
