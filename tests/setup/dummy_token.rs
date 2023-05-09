// use blockvisor_api::auth::{self, JwtToken};

// #[derive(serde::Serialize)]
// pub struct DummyToken<'a>(pub &'a str);

// impl<'a> JwtToken for DummyToken<'a> {
//     fn get_expiration(&self) -> i64 {
//         1_000_000
//     }

//     fn get_id(&self) -> uuid::Uuid {
//         uuid::Uuid::new_v4()
//     }

//     fn try_new(_: auth::TokenClaim) -> auth::TokenResult<Self> {
//         unimplemented!()
//     }

//     fn token_type(&self) -> auth::TokenType {
//         auth::TokenType::UserAuth
//     }
// }

// /// A dummy refresh token, which we use for those endpoints where we expect no refresh token to be
// /// needed.
// #[derive(serde::Serialize)]
// pub struct DummyRefresh;

// impl JwtToken for DummyRefresh {
//     fn get_expiration(&self) -> i64 {
//         panic!("Attempt to call `get_expiration` on a dummy refresh token")
//     }

//     fn get_id(&self) -> uuid::Uuid {
//         panic!("Attempt to call `get_id` on a dummy refresh token")
//     }

//     fn try_new(_: auth::TokenClaim) -> auth::TokenResult<Self> {
//         panic!("Attempt to call `try_new` on a dummy refresh token")
//     }

//     fn token_type(&self) -> auth::TokenType {
//         auth::TokenType::UserRefresh
//     }
// }
