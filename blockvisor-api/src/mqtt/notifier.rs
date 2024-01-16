use std::time::Duration;

use displaydoc::Display;
use prost::Message as _;
use rumqttc::v5::mqttbytes::v5::{Packet, Publish};
use rumqttc::v5::{AsyncClient, Event, MqttOptions};
use thiserror::Error;
use tracing::{trace, warn};

use crate::database::{Database, Pool};
use crate::grpc::api;
use crate::models::host::{ConnectionStatus, UpdateHost};

use super::{Client, Message, CLIENT_CAPACITY, CLIENT_QOS};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// MQTT client error: {0}
    Client(#[from] crate::mqtt::Error),
    /// Failed to parse HostId from MQTT HostStatus: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse HostStatus: {0}
    ParseHostStatus(prost::DecodeError),
    /// MQTT failed to get a pool connection: {0}
    PoolConnection(crate::database::Error),
    /// Failed to starting polling for events: {0}
    StartPolling(rumqttc::v5::ConnectionError),
    /// Failed to subscribe to `/bv/hosts/#`: {0}
    SubscribeHosts(rumqttc::v5::ClientError),
    /// MQTT failed to update host connection status: {0}
    UpdateHostStatus(crate::models::host::Error),
}

#[derive(Clone, Debug)]
pub struct Notifier {
    client: Client,
}

impl Notifier {
    pub async fn new(options: MqttOptions, pool: Pool) -> Result<Self, Error> {
        let (client, mut event_loop) = AsyncClient::new(options, CLIENT_CAPACITY);

        client
            .subscribe("$share/blockvisor-api//bv/hosts/#", CLIENT_QOS)
            .await
            .map_err(Error::SubscribeHosts)?;

        // poll event loop in the foreground until SubAck
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::SubAck(_))) => break,
                Ok(event) => trace!("startup MQTT event: {event:?}"),
                Err(err) => return Err(Error::StartPolling(err)),
            };
        }

        // then continue polling in the background and warn on errors
        tokio::spawn(async move {
            let pool = pool;
            loop {
                match event_loop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(packet))) => {
                        if let Err(err) = handle_packet(packet, &pool).await {
                            warn!("Failed to handle MQTT host event: {err}");
                        }
                    }
                    Ok(event) => trace!("incoming MQTT event: {event:?}"),
                    Err(err) => {
                        warn!("MQTT polling failure: {err}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        let client = Client::new(client);

        Ok(Self { client })
    }

    pub async fn send<M>(&self, message: M) -> Result<(), Error>
    where
        M: Into<Message> + Send,
    {
        self.client
            .clone()
            .send(message.into())
            .await
            .map_err(Into::into)
    }

    pub async fn send_all<I, M>(&self, messages: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = M> + Send,
        I::IntoIter: Send,
        M: Into<Message> + Send,
    {
        let mut client = self.client.clone();

        for msg in messages {
            client.send(msg.into()).await?;
        }

        Ok(())
    }
}

async fn handle_packet(packet: Publish, pool: &Pool) -> Result<(), Error> {
    let status = api::HostStatus::decode(&*packet.payload).map_err(Error::ParseHostStatus)?;
    let mut conn = pool.conn().await.map_err(Error::PoolConnection)?;

    UpdateHost::try_from(status)?
        .update(&mut conn)
        .await
        .map(|_host| ())
        .map_err(Error::UpdateHostStatus)
}

impl<'u> TryFrom<api::HostStatus> for UpdateHost<'u> {
    type Error = Error;

    fn try_from(status: api::HostStatus) -> Result<Self, Self::Error> {
        let id = status.host_id.parse().map_err(Error::ParseHostId)?;

        Ok(UpdateHost {
            id,
            status: Some(ConnectionStatus::from(status.connection_status())),
            name: None,
            version: None,
            cpu_count: None,
            mem_size_bytes: None,
            disk_size_bytes: None,
            os: None,
            os_version: None,
            ip_addr: None,
            ip_range_from: None,
            ip_range_to: None,
            ip_gateway: None,
            region_id: None,
            managed_by: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn can_subscribe_with_valid_credentials() {
        let context = Context::from_default_toml().await.unwrap();
        let options = context.config.mqtt.options().unwrap();

        let result = Notifier::new(options, context.pool.clone()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn will_fail_on_bad_credentials() {
        let context = Context::from_default_toml().await.unwrap();
        let context = Arc::into_inner(context).unwrap();
        let mut config = Arc::into_inner(context.config).unwrap();

        let mqtt = Arc::get_mut(&mut config.mqtt).unwrap();
        mqtt.username = "wrong".to_string();

        let result = Notifier::new(mqtt.options().unwrap(), context.pool.clone()).await;
        assert!(result.is_err());
    }
}
