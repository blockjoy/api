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
                // Blockjoy API
                "authentication.proto",
                "blockchain.proto",
                "command.proto",
                "discovery.proto",
                "host_provision.proto",
                "host.proto",
                "invitation.proto",
                "key_file.proto",
                "metrics.proto",
                "mqtt.proto",
                "node.proto",
                "organization.proto",
                "user.proto",
            ],
            &["cookbook_protos", "proto/v1"],
        )
    {
        eprintln!("Building protos failed with:\n{e}");
        std::process::exit(1);
    }
}
