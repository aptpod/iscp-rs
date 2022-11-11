fn main() {
    gen_proto();
}

#[cfg(not(feature = "gen"))]
fn gen_proto() {}

#[cfg(feature = "gen")]
fn gen_proto() {
    use glob::glob;

    const PROTO_SRC_DIR: &str = "iscp-proto/std";
    const PROTO_SRC_FILES: &str = "iscp-proto/std/*.proto";
    const AUTOGEN_DIR: &str = "src/encoding/internal/autogen";

    let proto_files: Vec<_> = glob(PROTO_SRC_FILES)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", PROTO_SRC_FILES, e))
        .filter_map(|res| res.ok())
        .collect();

    if std::path::Path::new(AUTOGEN_DIR).exists() {
        std::fs::remove_dir_all(AUTOGEN_DIR).expect("remove dir");
    }
    std::fs::create_dir(AUTOGEN_DIR).expect("create dir");

    prost_build::Config::new()
        .out_dir(AUTOGEN_DIR)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(
            "QoS",
            "#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive)]",
        )
        .type_attribute(
            "ResultCode",
            "#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive)]",
        )
        .type_attribute("DataID", "#[derive(PartialOrd, Ord, Eq, Hash)]")
        .type_attribute("DataFilter", "#[derive(PartialOrd, Ord, Eq, Hash)]")
        .compile_protos(&proto_files, &[PROTO_SRC_DIR])
        .expect("proto compile");
}
