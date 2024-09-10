use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::result::Error::NotFound;
use diesel::{ExpressionMethods, Insertable, QueryDsl, Queryable};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{NodeId, Resource, ResourceId, ResourceType};
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::schema::node_reports;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create report: {0}
    Create(diesel::result::Error),
    /// Failed to find node reports by id `{0}`: {1}
    FindByNode(NodeId, diesel::result::Error),
    /// Failed to find node reports by ids `{0:?}`: {1}
    FindByNodes(HashSet<NodeId>, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindByNode(_, NotFound) | FindByNodes(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    DieselNewType,
    Deref,
    From,
    FromStr,
)]
pub struct NodeReportId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct NodeReport {
    pub id: NodeReportId,
    pub node_id: NodeId,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

impl NodeReport {
    pub async fn by_node(node_id: NodeId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_reports::table
            .filter(node_reports::node_id.eq(node_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByNode(node_id, err))
    }

    pub async fn by_node_ids(
        node_ids: &HashSet<NodeId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        node_reports::table
            .filter(node_reports::node_id.eq_any(node_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByNodes(node_ids.clone(), err))
    }

    pub fn created_by(&self) -> Resource {
        Resource::new(self.created_by_type, self.created_by_id)
    }
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = node_reports)]
pub struct NewNodeReport {
    pub node_id: NodeId,
    pub created_by_type: ResourceType,
    pub created_by_id: ResourceId,
    pub message: String,
}

impl NewNodeReport {
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<NodeReport, Error> {
        let report = diesel::insert_into(node_reports::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)?;

        Ok(report)
    }
}
