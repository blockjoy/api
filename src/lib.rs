#![recursion_limit = "256"]
#![warn(
    rust_2018_idioms,
    rust_2021_compatibility,
    future_incompatible,
    nonstandard_style,
    unused,
    clippy::all,
    clippy::nursery,
    clippy::pedantic
)]
#![allow(
    clippy::default_trait_access,
    clippy::enum_glob_use,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::option_if_let_else,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::use_self
)]

#[macro_use]
extern crate maplit;

pub mod auth;
pub mod config;
pub mod cookbook;
pub mod database;
pub mod dns;
pub mod email;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod models;
pub mod mqtt;
pub mod server;
pub mod timestamp;
