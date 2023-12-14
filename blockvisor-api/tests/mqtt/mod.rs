mod publish;

use std::collections::VecDeque;
use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

use futures::channel::mpsc::{self, UnboundedReceiver};
use futures::{SinkExt, StreamExt};
use rand::distributions::{Alphanumeric, DistString};
use rumqttc::v5::mqttbytes::v5::{Packet, Publish};
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::{AsyncClient, Event, EventLoop, MqttOptions};
use tracing::debug;

use blockvisor_api::config::Config;
use blockvisor_api::mqtt::{CLIENT_CAPACITY, CLIENT_QOS};

const PACKET_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TestMqtt {
    pub client: AsyncClient,
    pub client_id: String,
    pub event_loop: EventLoop,
}

impl TestMqtt {
    pub fn new() -> Self {
        Self::new_with_options(default_options())
    }

    pub fn new_with_options(options: MqttOptions) -> Self {
        let client_id = options.client_id();
        let (client, event_loop) = AsyncClient::new(options, CLIENT_CAPACITY);

        TestMqtt {
            client,
            client_id,
            event_loop,
        }
    }

    pub fn topic(&self, suffix: &str) -> String {
        format!("/tests/{id}/{suffix}", id = self.client_id)
    }

    pub async fn subscribe(&mut self, topic: &str) {
        self.client.subscribe(topic, CLIENT_QOS).await.unwrap();

        loop {
            let event = self.event_loop.poll().await.unwrap();
            debug!("mqtt subscribe: {event:?}");
            if let Event::Incoming(Packet::SubAck(_)) = event {
                break;
            }
        }
    }

    pub async fn publish<P>(&mut self, topic: &str, payload: P)
    where
        P: Into<Vec<u8>>,
    {
        self.client
            .publish(topic, CLIENT_QOS, true, payload.into())
            .await
            .unwrap();

        loop {
            let event = self.event_loop.poll().await.unwrap();
            debug!("mqtt publish: {event:?}");

            match (event, CLIENT_QOS) {
                (Event::Incoming(Packet::PubAck(_)), QoS::AtLeastOnce) => break,
                (Event::Incoming(Packet::PubComp(_)), QoS::ExactlyOnce) => break,
                (_, QoS::AtMostOnce) => break,
                _ => (),
            }
        }
    }

    pub async fn next(&mut self) -> Publish {
        loop {
            let event = timeout(self.event_loop.poll()).await.unwrap();
            debug!("mqtt next: {event:?}");
            if let Event::Incoming(Packet::Publish(packet)) = event {
                return packet;
            }
        }
    }

    pub async fn try_next(&mut self) -> Option<Publish> {
        loop {
            let event =
                match tokio::time::timeout(Duration::from_secs(1), self.event_loop.poll()).await {
                    Ok(Ok(event)) => event,
                    Ok(Err(err)) => panic!("mqtt try_next: {err}"),
                    Err(_) => return None,
                };

            debug!("mqtt try_next: {event:?}");
            if let Event::Incoming(Packet::Publish(packet)) = event {
                return Some(packet);
            }
        }
    }
}

async fn timeout<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(PACKET_TIMEOUT, future).await.unwrap()
}

pub fn default_options() -> MqttOptions {
    let config = Config::from_default_toml().unwrap();
    let client_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
    let (server, port) = (config.mqtt.server_address.clone(), config.mqtt.server_port);

    let mut options = MqttOptions::new(client_id, server, port);
    options.set_credentials(&config.mqtt.username, &*config.mqtt.password);
    options.set_clean_start(true);
    options
}

pub async fn topic_messages(topic: &str) -> UnboundedReceiver<Publish> {
    let mut mqtt = TestMqtt::new();
    mqtt.subscribe(topic).await;

    let (mut packet_tx, packet_rx) = mpsc::unbounded();

    tokio::spawn(async move {
        loop {
            packet_tx.send(mqtt.next().await).await.unwrap();
        }
    });

    packet_rx
}

pub async fn assert_received<I, T>(topic: &str, payloads: I)
where
    I: IntoIterator<Item = T>,
    T: AsRef<[u8]> + Debug,
{
    let mut payloads = payloads.into_iter().collect::<VecDeque<_>>();
    let mut packet_rx = topic_messages(topic).await;

    while let Some(packet) = packet_rx.next().await {
        if packet.payload == payloads.front().unwrap().as_ref() {
            match std::str::from_utf8(&packet.payload) {
                Ok(text) => debug!("received payload: {text}"),
                Err(_) => debug!("received payload: {:x?}", packet.payload),
            };

            payloads.pop_front().unwrap();
            if payloads.is_empty() {
                break;
            }
        }
    }

    assert!(payloads.is_empty(), "Missing payloads: {payloads:?}");
}
