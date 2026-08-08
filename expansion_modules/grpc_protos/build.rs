use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = PathBuf::from("proto");

    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc_path);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                proto_dir.join("thebridge/common.proto"),
                proto_dir.join("thebridge/ai_ceo.proto"),
                proto_dir.join("thebridge/auth.proto"),
                proto_dir.join("thebridge/fiat.proto"),
                proto_dir.join("thebridge/health.proto"),
            ],
            &[proto_dir],
        )?;
    Ok(())
}
