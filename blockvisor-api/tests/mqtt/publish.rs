use rumqttc::v5::mqttbytes::v5::LastWill;

use super::{assert_received, default_options, TestMqtt};

#[tokio::test]
async fn mqtt_publish_and_subscribe() {
    let mut mqtt = TestMqtt::new();
    let topic = mqtt.topic("pub_sub");
    let payload = "hello";

    mqtt.publish(&topic, payload).await;
    assert_received(&topic, [payload]).await;
}

#[tokio::test]
async fn mqtt_last_will_on_drop() {
    let mut options = default_options();
    let client_id = options.client_id();

    let will_topic = format!("/tests/{client_id}/last_will_drop");
    let payload = format!("{client_id} disconnected");
    options.set_last_will(last_will(&will_topic, payload.as_bytes()));

    let mut mqtt = TestMqtt::new_with_options(options);
    mqtt.publish(&mqtt.topic("drop"), "hi").await;
    drop(mqtt);

    assert_received(&will_topic, [payload]).await;
}

#[tokio::test]
async fn mqtt_last_will_on_crash() {
    let mut options = default_options();
    let client_id = options.client_id();

    let will_topic = format!("/tests/{client_id}/last_will_crash");
    let payload = format!("{client_id} disconnected");
    options.set_last_will(last_will(&will_topic, payload.as_bytes()));

    tokio::spawn(async move {
        let mut mqtt = TestMqtt::new_with_options(options);
        let topic = mqtt.topic("crash");
        mqtt.publish(&topic, "hi").await;
        panic!("bye");
    });

    assert_received(&will_topic, [payload]).await;
}

#[tokio::test]
async fn mqtt_shared_topic() {
    let mut mqtt_pub = TestMqtt::new();
    let mut mqtt_sub1 = TestMqtt::new();
    let mut mqtt_sub2 = TestMqtt::new();

    let topic = mqtt_pub.topic("once");
    let shared = format!("$share/{id}/{topic}", id = mqtt_pub.client_id);

    mqtt_sub1.subscribe(&shared).await;
    mqtt_sub2.subscribe(&shared).await;

    let payload = "only once please";
    mqtt_pub.publish(&topic, payload).await;

    let packet1 = mqtt_sub1.try_next().await;
    let packet2 = mqtt_sub2.try_next().await;

    let packet = match (packet1, packet2) {
        (Some(packet), None) | (None, Some(packet)) => packet,
        (Some(_), Some(_)) => panic!("shared subscription got two packets"),
        (None, None) => panic!("shared subscription got no packets"),
    };

    assert_eq!(packet.payload, payload);
}

fn last_will<P: Into<Vec<u8>>>(topic: &str, payload: P) -> LastWill {
    use blockvisor_api::mqtt::CLIENT_QOS;
    LastWill::new(topic, payload, CLIENT_QOS, true, None)
}
