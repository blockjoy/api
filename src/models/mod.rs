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

diesel::sql_function!(fn lower(x: diesel::sql_types::Text) -> diesel::sql_types::Text);
diesel::sql_function!(fn string_to_array(version: diesel::sql_types::Text, split: diesel::sql_types::Text) -> diesel::sql_types::Array<diesel::sql_types::Text>);
