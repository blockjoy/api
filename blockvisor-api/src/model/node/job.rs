use derive_more::{From, IntoIterator};
use diesel::deserialize::{FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::sql_types::Jsonb;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{Output, ToSql};
use serde::{Deserialize, Serialize};

use crate::grpc::common;
use crate::util::HashVec;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeJob {
    pub name: String,
    pub status: Option<NodeJobStatus>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub logs: Vec<String>,
    pub restarts: u64,
    pub progress: Option<NodeJobProgress>,
}

impl From<NodeJob> for common::NodeJob {
    fn from(job: NodeJob) -> Self {
        common::NodeJob {
            name: job.name,
            status: job
                .status
                .map(common::NodeJobStatus::from)
                .unwrap_or_default()
                .into(),
            exit_code: job.exit_code,
            message: job.message,
            logs: job.logs,
            restarts: job.restarts,
            progress: job.progress.map(Into::into),
        }
    }
}

impl From<common::NodeJob> for NodeJob {
    fn from(job: common::NodeJob) -> Self {
        let status = job.status().into();

        NodeJob {
            name: job.name,
            status,
            exit_code: job.exit_code,
            message: job.message,
            logs: job.logs,
            restarts: job.restarts,
            progress: job.progress.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, AsExpression, From, FromSqlRow, IntoIterator, Serialize, Deserialize)]
#[diesel(sql_type = Jsonb)]
pub struct NodeJobs(pub Vec<NodeJob>);

impl NodeJobs {
    /// Merge this set of jobs with another by name, keeping this job for duplicates.
    #[must_use]
    pub fn merge(self, other: Option<NodeJobs>) -> Self {
        match other {
            Some(jobs) => {
                let merged = jobs
                    .into_iter()
                    .chain(self)
                    .to_map_keep_last(|job| (job.name.clone(), job))
                    .into_values()
                    .collect();
                NodeJobs(merged)
            }
            None => self,
        }
    }
}

impl FromSql<Jsonb, Pg> for NodeJobs {
    fn from_sql(value: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        serde_json::from_value(FromSql::<Jsonb, Pg>::from_sql(value)?).map_err(Into::into)
    }
}

impl ToSql<Jsonb, Pg> for NodeJobs {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        let json = serde_json::to_value(self).unwrap();
        <serde_json::Value as ToSql<Jsonb, Pg>>::to_sql(&json, &mut out.reborrow())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeJobStatus {
    Pending,
    Running,
    Finished,
    Failed,
    Stopped,
}

impl From<NodeJobStatus> for common::NodeJobStatus {
    fn from(status: NodeJobStatus) -> Self {
        match status {
            NodeJobStatus::Pending => Self::Pending,
            NodeJobStatus::Running => Self::Running,
            NodeJobStatus::Finished => Self::Finished,
            NodeJobStatus::Failed => Self::Failed,
            NodeJobStatus::Stopped => Self::Stopped,
        }
    }
}

impl From<common::NodeJobStatus> for Option<NodeJobStatus> {
    fn from(status: common::NodeJobStatus) -> Self {
        match status {
            common::NodeJobStatus::Unspecified => None,
            common::NodeJobStatus::Pending => Some(NodeJobStatus::Pending),
            common::NodeJobStatus::Running => Some(NodeJobStatus::Running),
            common::NodeJobStatus::Finished => Some(NodeJobStatus::Finished),
            common::NodeJobStatus::Failed => Some(NodeJobStatus::Failed),
            common::NodeJobStatus::Stopped => Some(NodeJobStatus::Stopped),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeJobProgress {
    pub total: Option<u32>,
    pub current: Option<u32>,
    pub message: Option<String>,
}

impl From<NodeJobProgress> for common::NodeJobProgress {
    fn from(progress: NodeJobProgress) -> Self {
        common::NodeJobProgress {
            total: progress.total,
            current: progress.current,
            message: progress.message,
        }
    }
}

impl From<common::NodeJobProgress> for NodeJobProgress {
    fn from(progress: common::NodeJobProgress) -> Self {
        NodeJobProgress {
            total: progress.total,
            current: progress.current,
            message: progress.message,
        }
    }
}
