mod setup;

use blockvisor_api::models;

#[tokio::test]
async fn can_blacklist_any_token() {
    let tester = setup::Tester::new().await;
    let token = "some-fancy-token".to_string();
    let model = models::BlacklistToken {
        token: token.clone(),
        token_type: models::TokenType::UserAuth,
    };
    let mut conn = tester.conn().await;
    let blt = model.create(&mut conn).await.unwrap();

    assert_eq!(blt.token, token);
}
