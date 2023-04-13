use super::api::FilteredIpAddr;
use crate::Error;
use crate::Result as ApiResult;
use anyhow::anyhow;
use prost_types::Timestamp;

/// Function to convert the datetimes from the database into the API representation of a timestamp.
pub fn try_dt_to_ts(datetime: chrono::DateTime<chrono::Utc>) -> crate::Result<Timestamp> {
    const NANOS_PER_SEC: i64 = 1_000_000_000;
    let nanos = datetime.timestamp_nanos();
    let timestamp = Timestamp {
        seconds: nanos / NANOS_PER_SEC,
        // This _should_ never fail because 1_000_000_000 fits into an i32, but using `as` was
        // hiding a bug here at first, therefore I have left the `try_into` call here.
        nanos: (nanos % NANOS_PER_SEC).try_into()?,
    };
    Ok(timestamp)
}

pub fn json_value_to_vec(json: &serde_json::Value) -> ApiResult<Vec<FilteredIpAddr>> {
    let arr = json
        .as_array()
        .ok_or_else(|| Error::UnexpectedError(anyhow!("Error deserializing JSON object")))?;
    let mut result = vec![];

    for value in arr {
        let tmp = value
            .as_object()
            .ok_or_else(|| Error::UnexpectedError(anyhow!("Error deserializing JSON array")))?;
        let ip = tmp
            .get("ip")
            .map(|e| e.to_string())
            .ok_or_else(|| Error::UnexpectedError(anyhow!("Can't read IP")))?
            .to_string();
        let description = tmp.get("description").map(|e| e.to_string());

        result.push(FilteredIpAddr { ip, description });
    }

    Ok(result)
}

// pub mod from {
//     use super::api::Keyfile;
//     use super::api_ui::blockchain_network::NetworkType;
//     use super::api_ui::BlockchainNetwork;
//     use super::api_ui::{
//         node::NodeStatus as GrpcNodeStatus, node::StakingStatus as GrpcStakingStatus,
//         node::SyncStatus as GrpcSyncStatus,
//     };
//     use crate::auth::{JwtToken, UserAuthToken};
//     use crate::cookbook::cookbook_grpc::NetworkConfiguration;
//     use crate::grpc;
//     use crate::models::{self, NodeChainStatus, NodeKeyFile, NodeStakingStatus, NodeSyncStatus};
//     use crate::Error;
//     use anyhow::anyhow;
//     use tonic::{Code, Status};

//     impl TryFrom<&UserAuthToken> for grpc::blockjoy_ui::ApiToken {
//         type Error = Error;

//         fn try_from(value: &UserAuthToken) -> Result<Self, Self::Error> {
//             Ok(Self {
//                 value: value.encode()?,
//             })
//         }
//     }

//     impl TryFrom<UserAuthToken> for grpc::blockjoy_ui::ApiToken {
//         type Error = Error;

//         fn try_from(value: UserAuthToken) -> Result<Self, Self::Error> {
//             Self::try_from(&value)
//         }
//     }

//     impl From<models::OrgUser> for grpc::blockjoy_ui::OrgUser {
//         fn from(value: models::OrgUser) -> Self {
//             Self {
//                 user_id: value.user_id.to_string(),
//                 org_id: value.org_id.to_string(),
//                 role: value.role as i32,
//             }
//         }
//     }

//     impl From<NodeChainStatus> for GrpcNodeStatus {
//         fn from(ncs: NodeChainStatus) -> Self {
//             match ncs {
//                 NodeChainStatus::Unknown => GrpcNodeStatus::Unspecified,
//                 NodeChainStatus::Provisioning => GrpcNodeStatus::Provisioning,
//                 NodeChainStatus::Broadcasting => GrpcNodeStatus::Broadcasting,
//                 NodeChainStatus::Cancelled => GrpcNodeStatus::Cancelled,
//                 NodeChainStatus::Delegating => GrpcNodeStatus::Delegating,
//                 NodeChainStatus::Delinquent => GrpcNodeStatus::Delinquent,
//                 NodeChainStatus::Disabled => GrpcNodeStatus::Disabled,
//                 NodeChainStatus::Earning => GrpcNodeStatus::Earning,
//                 NodeChainStatus::Elected => GrpcNodeStatus::Elected,
//                 NodeChainStatus::Electing => GrpcNodeStatus::Electing,
//                 NodeChainStatus::Exported => GrpcNodeStatus::Exported,
//                 NodeChainStatus::Ingesting => GrpcNodeStatus::Ingesting,
//                 NodeChainStatus::Mining => GrpcNodeStatus::Mining,
//                 NodeChainStatus::Minting => GrpcNodeStatus::Minting,
//                 NodeChainStatus::Processing => GrpcNodeStatus::Processing,
//                 NodeChainStatus::Relaying => GrpcNodeStatus::Relaying,
//                 NodeChainStatus::Removed => GrpcNodeStatus::Removed,
//                 NodeChainStatus::Removing => GrpcNodeStatus::Removing,
//             }
//         }
//     }

//     impl From<NodeSyncStatus> for GrpcSyncStatus {
//         fn from(nss: NodeSyncStatus) -> Self {
//             match nss {
//                 NodeSyncStatus::Unknown => GrpcSyncStatus::Unspecified,
//                 NodeSyncStatus::Synced => GrpcSyncStatus::Synced,
//                 NodeSyncStatus::Syncing => GrpcSyncStatus::Syncing,
//             }
//         }
//     }

//     impl From<NodeStakingStatus> for GrpcStakingStatus {
//         fn from(nss: NodeStakingStatus) -> Self {
//             match nss {
//                 NodeStakingStatus::Unknown => GrpcStakingStatus::Unspecified,
//                 NodeStakingStatus::Staked => GrpcStakingStatus::Staked,
//                 NodeStakingStatus::Staking => GrpcStakingStatus::Staking,
//                 NodeStakingStatus::Validating => GrpcStakingStatus::Validating,
//                 NodeStakingStatus::Follower => GrpcStakingStatus::Follower,
//                 NodeStakingStatus::Consensus => GrpcStakingStatus::Consensus,
//                 NodeStakingStatus::Unstaked => GrpcStakingStatus::Unstaked,
//             }
//         }
//     }

//     impl TryFrom<NodeKeyFile> for Keyfile {
//         type Error = Error;

//         fn try_from(value: NodeKeyFile) -> Result<Self, Self::Error> {
//             Ok(Self {
//                 name: value.name.clone(),
//                 content: value.content.into_bytes(),
//             })
//         }
//     }
// }
