mod setup;

use api::models::{IpAddress, NewIpAddressRange};

#[tokio::test]
async fn should_create_ip_range() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    let new_range = NewIpAddressRange::try_new(
        "192.129.0.10".parse().unwrap(),
        "192.129.0.20".parse().unwrap(),
        Some(host.id),
    )?;
    let mut conn = tester.conn().await;
    let range = new_range.create(&mut conn).await?;
    assert_eq!(range.len(), 11);

    Ok(())
}

#[tokio::test]
#[should_panic]
async fn should_fail_creating_ip_range() {
    let tester = setup::Tester::new().await;
    let host = tester.host().await;
    NewIpAddressRange::try_new(
        "192.129.0.20".parse().unwrap(),
        "192.129.0.10".parse().unwrap(),
        Some(host.id),
    )
    .unwrap();
}

#[test]
fn should_fail_if_ip_in_range() {
    let ref_ip = "192.168.0.15".parse().unwrap();
    let from_ip = "192.168.0.10".parse().unwrap();
    let to_ip = "192.168.0.10".parse().unwrap();

    assert!(!IpAddress::in_range(ref_ip, from_ip, to_ip));
}
