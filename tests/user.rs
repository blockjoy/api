mod setup;

use api::auth::{JwtToken, TokenClaim, TokenRole, TokenType, UserAuthToken, UserRefreshToken};
use api::models::{UpdateUser, User};
use chrono::Utc;

#[tokio::test]
async fn can_verify_and_refresh_auth_token() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let claim = TokenClaim::new(
        user.id,
        Utc::now().timestamp() + 60000,
        TokenType::UserRefresh,
        TokenRole::User,
        None,
    );
    let refresh_token = UserRefreshToken::try_new(claim)?;
    let encoded = refresh_token.encode()?;
    let fields = UpdateUser {
        refresh: Some(&encoded),
        ..Default::default()
    };
    let mut conn = tester.conn().await;
    let user = fields.update(&mut conn).await?;
    let claim = TokenClaim::new(
        user.id,
        Utc::now().timestamp() - 1,
        TokenType::UserAuth,
        TokenRole::User,
        None,
    );
    let auth = UserAuthToken::try_new(claim)?;

    User::verify_and_refresh_auth_token(auth, refresh_token, &mut conn)
        .await
        .unwrap();
    Ok(())
}

#[tokio::test]
async fn cannot_verify_and_refresh_wo_valid_refresh_token() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;
    let claim = TokenClaim::new(
        user.id,
        Utc::now().timestamp() - 60000,
        TokenType::UserRefresh,
        TokenRole::User,
        None,
    );
    let refresh_token = UserRefreshToken::try_new(claim)?;
    let encoded = refresh_token.encode()?;
    let fields = UpdateUser {
        refresh: Some(&encoded),
        ..Default::default()
    };
    let mut conn = tester.conn().await;
    let user = fields.update(&mut conn).await?;
    let claim = TokenClaim::new(
        user.id,
        Utc::now().timestamp() - 1,
        TokenType::UserAuth,
        TokenRole::User,
        None,
    );
    let auth_token = UserAuthToken::try_new(claim)?;

    User::verify_and_refresh_auth_token(auth_token, refresh_token, &mut conn)
        .await
        .unwrap_err();

    Ok(())
}

#[tokio::test]
async fn can_confirm_unconfirmed_user() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = User::confirm(user.id, &mut conn).await?;

    user.confirmed_at.unwrap();

    Ok(())
}

#[tokio::test]
async fn cannot_confirm_confirmed_user() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = User::confirm(user.id, &mut conn).await?;

    assert!(user.confirmed_at.is_some());

    User::confirm(user.id, &mut conn)
        .await
        .expect_err("Already confirmed user confirmed again");
    Ok(())
}

#[tokio::test]
async fn can_check_if_user_confirmed() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());

    let mut conn = tester.conn().await;
    let user = User::confirm(user.id, &mut conn).await?;

    assert!(user.confirmed_at.is_some());
    assert!(User::is_confirmed(user.id, &mut conn).await?);

    Ok(())
}

#[tokio::test]
async fn returns_false_for_unconfirmed_user_at_check_if_user_confirmed() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let user = tester.admin_user().await;

    assert!(user.confirmed_at.is_none());
    let mut conn = tester.conn().await;
    assert!(!User::is_confirmed(user.id, &mut conn).await?);

    Ok(())
}
