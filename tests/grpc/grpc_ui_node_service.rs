use blockvisor_api::grpc::api::{self, nodes_client};
use tonic::transport;

type Service = nodes_client::NodeServiceClient<transport::Channel>;
