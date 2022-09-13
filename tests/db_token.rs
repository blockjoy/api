mod setup;

use crate::setup::get_admin_user;
use api::auth::{AuthToken, JwtToken, TokenHolderType, TokenIdentifyable};
use api::models::Token;
use setup::{get_test_host, setup};
use std::thread::sleep;
use std::time::Duration;
use test_macros::*;

#[before(call = "setup")]
#[tokio::test]
async fn can_create_host_token() {
    let db = _before_values.await;
    let host = get_test_host(&db).await;
    let token = host.get_token(&db).await.unwrap();
    let token_str = AuthToken::new(host.id, token.expires_at.timestamp(), TokenHolderType::Host)
        .encode()
        .unwrap();

    assert_eq!(token.token, token_str);
}

#[before(call = "setup")]
#[tokio::test]
async fn can_refresh_host_token() {
    let db = _before_values.await;
    let host = get_test_host(&db).await;
    let token = host.get_token(&db).await.unwrap();

    // sleep 1 sec so the expiration REALLY changes
    sleep(Duration::from_secs(1));

    match Token::refresh(token.token, &db).await {
        Ok(_) => println!("All good"),
        Err(e) => panic!("error at refresh: {}", e),
    }
}

#[before(call = "setup")]
#[tokio::test]
async fn can_create_user_token() {
    let db = _before_values.await;
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();
    let token_str = AuthToken::new(user.id, token.expires_at.timestamp(), TokenHolderType::User)
        .encode()
        .unwrap();

    assert_eq!(token.token, token_str);
}

#[before(call = "setup")]
#[tokio::test]
async fn can_refresh_user_token() {
    let db = _before_values.await;
    let user = get_admin_user(&db).await;
    let token = user.get_token(&db).await.unwrap();

    // sleep 1 sec so the expiration REALLY changes
    sleep(Duration::from_secs(1));

    match Token::refresh(token.token, &db).await {
        Ok(_) => println!("All good"),
        Err(e) => panic!("error at refresh: {}", e),
    }
}
