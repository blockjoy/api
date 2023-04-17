use blockvisor_api::grpc::api::authentication_client;
use blockvisor_api::grpc::api::blockchains_client;
use blockvisor_api::grpc::api::commands_client;
use blockvisor_api::grpc::api::discovery_client;
use blockvisor_api::grpc::api::host_provisions_client;
use blockvisor_api::grpc::api::hosts_client;
use blockvisor_api::grpc::api::invitations_client;
use blockvisor_api::grpc::api::key_files_client;
use blockvisor_api::grpc::api::metrics_client;
use blockvisor_api::grpc::api::nodes_client;
use blockvisor_api::grpc::api::orgs_client;
use blockvisor_api::grpc::api::users_client;
use tonic::transport::Channel;

pub trait GrpcClient<T> {
    fn create(channel: Channel) -> Self;
}

impl GrpcClient<Channel> for authentication_client::AuthenticationClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for blockchains_client::BlockchainsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for commands_client::CommandsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for discovery_client::DiscoveryClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for host_provisions_client::HostProvisionsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for hosts_client::HostsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for invitations_client::InvitationsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for key_files_client::KeyFilesClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for metrics_client::MetricsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for nodes_client::NodesClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for orgs_client::OrgsClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}

impl GrpcClient<Channel> for users_client::UsersClient<Channel> {
    fn create(channel: Channel) -> Self {
        Self::new(channel)
    }
}
