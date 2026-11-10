fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("REGENERATE_PROTO").is_err() {
        return Ok(());
    }

    tonic_prost_build::configure()
        .out_dir("src/generated")
        .compile_protos(
            &[
                "protos/auth.proto",
                "protos/packet.proto",
                "protos/shared.proto",
                "protos/bundle.proto",
                "protos/searcher.proto",
            ],
            &["protos"],
        )?;

    println!("cargo:rerun-if-changed=protos");
    Ok(())
}
