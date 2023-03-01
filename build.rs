//! Build file generating gRPC stubs

fn main() {
    if let Err(e) = tonic_build::configure()
        .build_server(true)
        // needed for integration tests
        .build_client(true)
        .compile(
            &[
                // Cookbook API
                "service.proto",
                // Backend API
                "discovery.proto",
                "host_service.proto",
                "node_service.proto",
                "key_file_service.proto",
                "metrics.proto",
                "command.proto",
                // UI API
                "authentication_service.proto",
                "billing_service.proto",
                "blockchain_service.proto",
                "command_service.proto",
                "dashboard_service.proto",
                "host_provision_service.proto",
                "invitation_service.proto",
                "fe_host_service.proto",
                "ui_node_service.proto",
                "organization_service.proto",
                "user_service.proto",
            ],
            &[
                "cookbook_protos",
                "proto/blockjoy/api/v1",
                "proto/blockjoy/api/ui_v1",
            ],
        )
    {
        eprintln!("Building protos failed with:\n{e}");
        std::process::exit(1);
    }
}
