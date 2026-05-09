use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let proto_dir = manifest_dir.join("proto");

    println!("cargo:rerun-if-changed={}", proto_dir.display());

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto_dir.join("runtime.proto"), proto_dir.join("agent.proto")], &[proto_dir])
        .expect("compile platform grpc protos");
}
