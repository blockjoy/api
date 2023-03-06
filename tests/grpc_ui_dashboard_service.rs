mod setup;

use api::grpc::blockjoy_ui::{self, dashboard_service_client, metric};
use tonic::transport;

type Service = dashboard_service_client::DashboardServiceClient<transport::Channel>;

#[tokio::test]
async fn responds_unauthenticated_with_invalid_token_for_metrics() {
    let tester = setup::Tester::new().await;
    let req = blockjoy_ui::DashboardMetricsRequest {
        meta: Some(tester.meta()),
        org_id: tester.org().await.id.to_string(),
    };
    let (auth, refresh) = (setup::DummyToken("some-invalid-token"), setup::DummyRefresh);
    let status = tester
        .send_with(Service::metrics, req, auth, refresh)
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn responds_ok_with_valid_token_for_metrics() {
    let tester = setup::Tester::new().await;
    let admin = tester.admin_user().await;
    let org = tester.org_for(&admin).await;
    let req = blockjoy_ui::DashboardMetricsRequest {
        meta: Some(tester.meta()),
        org_id: org.id.to_string(),
    };
    tester.send_admin(Service::metrics, req).await.unwrap();
}

#[tokio::test]
async fn responds_valid_values_for_metrics() {
    let tester = setup::Tester::new().await;
    let admin = tester.admin_user().await;
    let org = tester.org_for(&admin).await;
    let req = blockjoy_ui::DashboardMetricsRequest {
        meta: Some(tester.meta()),
        org_id: org.id.to_string(),
    };
    let resp = tester.send_admin(Service::metrics, req).await.unwrap();
    let metrics = resp.metrics;

    let online_name: i32 = metrics.first().unwrap().name;
    let offline_name: i32 = metrics.last().unwrap().name;
    let online_value: i32 = metrics.first().unwrap().value.parse().unwrap();
    let offline_value: i32 = metrics.last().unwrap().value.parse().unwrap();

    assert_eq!(online_name, metric::Name::Online as i32);
    assert_eq!(offline_name, metric::Name::Offline as i32);
    assert_eq!(online_value, 1);
    assert_eq!(offline_value, 0);
}
