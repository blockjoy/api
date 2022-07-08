//! Build file generating gRPC stubs

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .compile(&["proto/api/api_services.proto"], &["proto", "proto/api"])?;

    Ok(())
}
