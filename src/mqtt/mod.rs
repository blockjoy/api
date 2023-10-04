//! Presents the following senders:
//!
//! ```text
//! |---------------|----------------------------------------------|
//! | public api    | topics                                       |
//! |---------------|----------------------------------------------|
//! | organizations | /orgs/<org_id>                               |
//! |---------------|----------------------------------------------|
//! | hosts         | /hosts/<host_id>                             |
//! |---------------|----------------------------------------------|
//! | nodes         | /orgs/<org_id>/nodes                         |
//! |               | /hosts/<host_id>/nodes                       |
//! |               | /nodes/<node_id>                             |
//! |---------------|----------------------------------------------|
//! | commands      | /hosts/<host_id>/nodes/<node_id>/commands    |
//! |               | /hosts/<host_id>/commands                    |
//! |               | /nodes/<node_id>/commands                    |
//! |---------------|----------------------------------------------|
//! ```

pub mod handler;

pub mod message;
pub use message::Message;

pub mod notifier;
pub use notifier::Notifier;
