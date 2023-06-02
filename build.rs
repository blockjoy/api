//! Build file generating gRPC stubs

fn main() {
    if let Err(e) = tonic_build::configure()
        .build_server(true)
        // needed for integration tests
        .build_client(true)
        .compile(
            &[
                "blockjoy/v1/auth.proto",
                "blockjoy/v1/babel.proto",
                "blockjoy/v1/blockchain.proto",
                "blockjoy/v1/command.proto",
                "blockjoy/v1/cookbook.proto",
                "blockjoy/v1/discovery.proto",
                "blockjoy/v1/host.proto",
                "blockjoy/v1/invitation.proto",
                "blockjoy/v1/key_file.proto",
                "blockjoy/v1/metrics.proto",
                "blockjoy/v1/mqtt.proto",
                "blockjoy/v1/node.proto",
                "blockjoy/v1/org.proto",
                "blockjoy/v1/user.proto",
            ],
            &["proto"],
        )
    {
        eprintln!("Building protos failed with:\n{e}");
        std::process::exit(1);
    }
}
