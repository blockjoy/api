use api::grpc::blockjoy::commands_client::CommandsClient;
use api::grpc::blockjoy::hosts_client::HostsClient;
use api::grpc::blockjoy::key_files_client::KeyFilesClient;
use api::grpc::blockjoy::metrics_service_client::MetricsServiceClient;
use api::grpc::blockjoy::nodes_client::NodesClient;
use api::grpc::blockjoy_ui::authentication_service_client::AuthenticationServiceClient;
use api::grpc::blockjoy_ui::blockchain_service_client::BlockchainServiceClient;
use api::grpc::blockjoy_ui::command_service_client::CommandServiceClient;
use api::grpc::blockjoy_ui::dashboard_service_client::DashboardServiceClient;
use api::grpc::blockjoy_ui::host_provision_service_client::HostProvisionServiceClient;
use api::grpc::blockjoy_ui::host_service_client::HostServiceClient;
use api::grpc::blockjoy_ui::invitation_service_client::InvitationServiceClient;
use api::grpc::blockjoy_ui::node_service_client::NodeServiceClient;
use api::grpc::blockjoy_ui::organization_service_client::OrganizationServiceClient;
use api::grpc::blockjoy_ui::user_service_client::UserServiceClient;
use tonic::transport::Channel;

pub trait GrpcClient<T> {
    fn create(channel: Channel) -> Self;
}

impl GrpcClient<Channel> for HostsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for AuthenticationServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for OrganizationServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for UserServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for HostServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for HostProvisionServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for NodeServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for CommandServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for DashboardServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for BlockchainServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for KeyFilesClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for MetricsServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for InvitationServiceClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for NodesClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for CommandsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}
