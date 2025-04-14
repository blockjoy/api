use std::sync::Arc;
use std::time::Duration;

use displaydoc::Display;
use prost::Message as _;
use rumqttc::v5::mqttbytes::v5::{Packet, Publish};
use rumqttc::v5::{AsyncClient, Event, MqttOptions};
use thiserror::Error;
use tracing::{trace, warn};

use crate::database::{Database, Pool};
use crate::grpc::command::host_pending;
use crate::grpc::common;
use crate::model::command::NewCommand;
use crate::model::host::{ConnectionStatus, UpdateHost};
use crate::model::{Command, CommandType};

use super::{CLIENT_CAPACITY, CLIENT_QOS, Client, Message};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// MQTT client error: {0}
    Client(#[from] crate::mqtt::Error),
    /// MQTT Command error: {0}
    Command(#[from] crate::model::command::Error),
    /// MQTT GRPC Command error: {0}
    GrpcCommand(#[from] crate::grpc::command::Error),
    /// MQTT host error: {0}
    Host(#[from] crate::model::host::Error),
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
    UpdateHostStatus(crate::model::host::Error),
}

#[derive(Clone, Debug)]
pub struct Notifier {
    client: Client,
}

impl Notifier {
    pub async fn new(options: MqttOptions, pool: Pool) -> Result<Arc<Self>, Error> {
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
            }
        }

        let client = Client::new(client);
        let notifier = Arc::new(Self { client });
        let mqtt = notifier.clone();

        // then continue polling in the background and warn on errors
        tokio::spawn(async move {
            let pool = pool;
            let mqtt = mqtt;

            loop {
                match event_loop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(packet))) => {
                        if let Err(err) = mqtt.handle_packet(packet, &pool).await {
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

        Ok(notifier)
    }

    pub async fn send<M>(&self, message: M) -> Result<(), Error>
    where
        M: Into<Message> + Send,
    {
        let message = message.into();
        self.client.clone().send(message).await.map_err(Into::into)
    }

    async fn handle_packet(&self, packet: Publish, pool: &Pool) -> Result<(), Error> {
        let status =
            common::HostStatus::decode(&*packet.payload).map_err(Error::ParseHostStatus)?;
        let mut conn = pool.conn().await.map_err(Error::PoolConnection)?;

        let host_id = status.host_id.parse().map_err(Error::ParseHostId)?;
        let conn_status = status.connection_status().try_into()?;

        UpdateHost::default()
            .with_connection_status(conn_status)
            .apply(host_id, &mut conn)
            .await
            .map_err(Error::UpdateHostStatus)?;

        if conn_status == ConnectionStatus::Online
            && Command::has_host_pending(host_id, &mut conn).await?
        {
            let pending = NewCommand::host(host_id, CommandType::HostPending)?;
            let command = pending.create(&mut conn).await?;
            let api_cmd = host_pending(&command)?;
            self.send(api_cmd).await?;
        }

        Ok(())
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

    #[ignore]
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
