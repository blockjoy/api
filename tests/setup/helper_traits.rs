use blockvisor_api::grpc::api::auth_service_client;
use blockvisor_api::grpc::api::blockchain_service_client;
use blockvisor_api::grpc::api::command_service_client;
use blockvisor_api::grpc::api::discovery_service_client;
use blockvisor_api::grpc::api::host_provision_service_client;
use blockvisor_api::grpc::api::host_service_client;
use blockvisor_api::grpc::api::invitation_service_client;
use blockvisor_api::grpc::api::key_file_service_client;
use blockvisor_api::grpc::api::metrics_service_client;
use blockvisor_api::grpc::api::node_service_client;
use blockvisor_api::grpc::api::org_service_client;
use blockvisor_api::grpc::api::user_service_client;
use tonic::transport::Channel;

pub trait GrpcClient<T> {
    fn create(channel: Channel) -> Self;
}

impl GrpcClient<Channel> for auth_service_client::AuthServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for blockchain_service_client::BlockchainServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for command_service_client::CommandServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for discovery_service_client::DiscoveryServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for host_provision_service_client::HostProvisionServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for host_service_client::HostServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for invitation_service_client::InvitationServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for key_file_service_client::KeyFileServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for metrics_service_client::MetricsServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for node_service_client::NodeServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for org_service_client::OrgServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for user_service_client::UserServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}
