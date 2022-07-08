use tonic::{metadata::MetadataValue, Request, Status};

pub mod blockjoy {
    tonic::include_proto!("blockjoy.api");
}
