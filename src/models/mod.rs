//! This module contains the code related to database tables. Each table is represented by one or
//! more models, which are structs that have a field for the columns of the queries that they
//! interact with. There may exist multiple models for a given table, for example models that are
//! used for updating rows often do not contain all of the columns, whereas models that are used
//! for selecting usually do.

pub mod api_key;
pub use api_key::ApiKey;

pub mod blacklist_token;
pub use blacklist_token::BlacklistToken;

pub mod blockchain;
pub use blockchain::Blockchain;

pub mod command;
pub use command::{Command, CommandType};

pub mod host;
pub use host::{ConnectionStatus, Host, HostRequirements, HostType};

pub mod invitation;
pub use invitation::Invitation;

pub mod ip_address;
pub use ip_address::IpAddress;

pub mod node;
pub use node::{
    ContainerStatus, Node, NodeChainStatus, NodeProperty, NodeStakingStatus, NodeSyncStatus,
};

pub mod node_key_file;
pub use node_key_file::NodeKeyFile;

pub mod node_log;
pub use node_log::{NodeLog, NodeLogEvent};

pub mod node_scheduler;
pub use node_scheduler::{NodeScheduler, ResourceAffinity, SimilarNodeAffinity};

pub mod node_type;
pub use node_type::NodeType;

pub mod org;
pub use org::{Org, OrgRole, OrgUser};

pub mod paginate;
pub use paginate::Paginate;

pub mod region;
pub use region::Region;

pub mod schema;

pub mod subscription;
pub use subscription::{Subscription, SubscriptionId};

pub mod user;
pub use user::User;

use std::cmp;

diesel::sql_function!(fn lower(x: diesel::sql_types::Text) -> diesel::sql_types::Text);
diesel::sql_function!(fn string_to_array(version: diesel::sql_types::Text, split: diesel::sql_types::Text) -> diesel::sql_types::Array<diesel::sql_types::Text>);

fn semver_cmp(s1: &str, s2: &str) -> Option<cmp::Ordering> {
    s1.split('.')
        .zip(s2.split('.'))
        .find_map(|(s1, s2)| cmp_str(s1, s2))
}

fn cmp_str(s1: &str, s2: &str) -> Option<cmp::Ordering> {
    let take_nums = |s: &str| s.chars().take_while(|c| c.is_numeric()).collect::<String>();
    let parse_nums = |s: String| s.parse().ok();
    parse_nums(take_nums(s1))
        .and_then(|n1: i64| parse_nums(take_nums(s2)).map(move |n2| (n1, n2)))
        .map(|(n1, n2)| n1.cmp(&n2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmp::Ordering::*;

    #[test]
    fn test_semver_cmp() {
        assert_eq!(semver_cmp("1.2.3", "1.2.3"), Some(Equal));
        assert_eq!(semver_cmp("3.2.3", "1.2.3"), Some(Greater));
        assert_eq!(semver_cmp("1.2.3", "3.2.3"), Some(Less));
        assert_eq!(semver_cmp("1.2.3-beta", "1.2.3"), Some(Equal));
        assert_eq!(semver_cmp("1.2.3-beta3", "1.2.3.4"), Some(Equal));
        assert_eq!(semver_cmp("1.2-beta.3", "1.2"), Some(Equal));
    }

    #[test]
    fn test_cmp_str() {
        assert_eq!(cmp_str("1", "1"), Some(Equal));
        assert_eq!(cmp_str("1", "2"), Some(Less));
        assert_eq!(cmp_str("3", "2"), Some(Greater));
        assert_eq!(cmp_str("3-beta", "2"), Some(Greater));
        assert_eq!(cmp_str("3", "2-beta"), Some(Greater));
    }
}
