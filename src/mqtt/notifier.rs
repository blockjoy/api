use std::time::Duration;

use displaydoc::Display;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use thiserror::Error;
use tracing::{trace, warn};

use super::Message;

const CLIENT_CAPACITY: usize = 10;
const CLIENT_QOS: QoS = QoS::ExactlyOnce;
const CLIENT_RETAIN: bool = false;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to get Message channels: {0}
    Channels(super::message::Error),
    /// Critical MQTT connection error: {0}
    Critical(rumqttc::ConnectionError),
    /// Failed to publish Message: {0}
    Publish(rumqttc::ClientError),
    /// Failed to starting polling for events: {0}
    StartPolling(rumqttc::ConnectionError),
    /// Failed to subscribe to `/bv/hosts/#`: {0}
    SubscribeHosts(rumqttc::ClientError),
}

#[derive(Clone, Debug)]
pub struct Notifier {
    client: Client,
}

impl Notifier {
    pub async fn new(options: MqttOptions) -> Result<Self, Error> {
        let (client, mut event_loop) = AsyncClient::new(options, CLIENT_CAPACITY);

        client
            .subscribe("/bv/hosts/#", QoS::AtLeastOnce)
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
            loop {
                match event_loop.poll().await {
                    Ok(event) => trace!("received MQTT event: {event:?}"),
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
        self.client.clone().send(message.into()).await
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::config::Config;

    use super::*;

    #[tokio::test]
    async fn can_subscribe_with_valid_credentials() {
        let config = Config::from_default_toml().unwrap();

        let result = Notifier::new(config.mqtt.options()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn will_fail_on_bad_credentials() {
        let mut config = Config::from_default_toml().unwrap();
        let mqtt = Arc::get_mut(&mut config.mqtt).unwrap();
        mqtt.username = "wrong".to_string();

        let result = Notifier::new(mqtt.options()).await;
        assert!(result.is_err());
    }
}
