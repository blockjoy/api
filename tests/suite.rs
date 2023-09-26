//! Integration testing suite in one module so they can be ran in parallel.

mod auth;
mod grpc;
mod http;
mod setup;

#[macro_use]
extern crate maplit;
