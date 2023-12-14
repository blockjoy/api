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

use displaydoc::Display;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::AsyncClient;
use thiserror::Error;

pub const CLIENT_CAPACITY: usize = 10;
pub const CLIENT_QOS: QoS = QoS::AtLeastOnce;
pub const CLIENT_RETAIN: bool = false;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to get Message channels: {0}
    Channels(self::message::Error),
    /// Failed to publish Message: {0}
    Publish(rumqttc::v5::ClientError),
}

#[derive(Clone, Debug)]
pub struct Client {
    client: AsyncClient,
}

impl Client {
    const fn new(client: AsyncClient) -> Self {
        Self { client }
    }

    pub async fn send(&mut self, msg: Message) -> Result<(), Error> {
        let payload = msg.encode();
        let channels = msg.channels().map_err(Error::Channels)?;

        for channel in channels {
            self.client
                .publish(&channel, CLIENT_QOS, CLIENT_RETAIN, payload.clone())
                .await
                .map_err(Error::Publish)?;
        }

        Ok(())
    }
}
