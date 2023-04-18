//! This module contains a set of sub-modules that each test a gRPC service as defined in our api
//! definitions. Since they all take a reasonable amount of time (because they require significant
//! amounts of setup work, including creating a database) they are all included in this module.
//! Tests in a single module can be ran in parallel, so this makes our test suite run faster.

mod grpc;
mod setup;
