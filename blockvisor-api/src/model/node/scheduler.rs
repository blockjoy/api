use diesel_derive_enum::DbEnum;

use crate::grpc::common;
use crate::model::schema::sql_types;
use crate::model::Region;

/// Controls how a node is placed onto an appropriate host.
#[derive(Debug)]
pub struct NodeScheduler {
    /// Affinity to scheduling on the most or least heavily utilized hosts.
    pub resource: ResourceAffinity,
    /// Affinity to similar nodes on a host. Takes precedence over `resource`.
    pub similarity: Option<SimilarNodeAffinity>,
    /// The region for the node. Takes precedence over `similarity`.
    pub region: Option<Region>,
}

impl NodeScheduler {
    pub const fn least_resources() -> Self {
        NodeScheduler {
            resource: ResourceAffinity::LeastResources,
            similarity: None,
            region: None,
        }
    }
}

impl From<NodeScheduler> for common::NodeScheduler {
    fn from(scheduler: NodeScheduler) -> Self {
        common::NodeScheduler {
            similarity: scheduler
                .similarity
                .map(common::SimilarNodeAffinity::from)
                .map(Into::into),
            resource: common::ResourceAffinity::from(scheduler.resource).into(),
            region: scheduler.region.map(|r| r.name),
        }
    }
}

/// Whether similar nodes will be placed on the same host or spread over many.
#[derive(Clone, Copy, Debug, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeSimilarityAffinity"]
pub enum SimilarNodeAffinity {
    /// Schedule similar nodes on the same cluster (e.g. for low latency).
    Cluster,
    /// Avoid scheduling on hosts running similar nodes (e.g. for redundancy).
    Spread,
}

impl From<SimilarNodeAffinity> for common::SimilarNodeAffinity {
    fn from(affinity: SimilarNodeAffinity) -> Self {
        match affinity {
            SimilarNodeAffinity::Cluster => common::SimilarNodeAffinity::Cluster,
            SimilarNodeAffinity::Spread => common::SimilarNodeAffinity::Spread,
        }
    }
}

impl From<common::SimilarNodeAffinity> for Option<SimilarNodeAffinity> {
    fn from(affinity: common::SimilarNodeAffinity) -> Self {
        match affinity {
            common::SimilarNodeAffinity::Unspecified => None,
            common::SimilarNodeAffinity::Cluster => Some(SimilarNodeAffinity::Cluster),
            common::SimilarNodeAffinity::Spread => Some(SimilarNodeAffinity::Spread),
        }
    }
}

/// Whether nodes will be scheduled on the most or least heavily utilized hosts.
#[derive(Clone, Copy, Debug, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeResourceAffinity"]
pub enum ResourceAffinity {
    /// Prefer to utilize full hosts first.
    MostResources,
    /// Prefer to utilize empty hosts first.
    LeastResources,
}

impl From<ResourceAffinity> for common::ResourceAffinity {
    fn from(affinity: ResourceAffinity) -> Self {
        match affinity {
            ResourceAffinity::MostResources => common::ResourceAffinity::MostResources,
            ResourceAffinity::LeastResources => common::ResourceAffinity::LeastResources,
        }
    }
}

impl From<common::ResourceAffinity> for Option<ResourceAffinity> {
    fn from(affinity: common::ResourceAffinity) -> Self {
        match affinity {
            common::ResourceAffinity::Unspecified => None,
            common::ResourceAffinity::MostResources => Some(ResourceAffinity::MostResources),
            common::ResourceAffinity::LeastResources => Some(ResourceAffinity::LeastResources),
        }
    }
}
