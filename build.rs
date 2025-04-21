use anyhow::Result;

#[cfg(feature = "gen")]
use anyhow::Context;

fn main() -> Result<()> {
    gen_proto()
}

#[cfg(not(feature = "gen"))]
fn gen_proto() -> Result<()> {
    Ok(())
}

#[cfg(feature = "gen")]
const PROTO_SRC_DIR: &str = "iscp-proto/proto";

#[cfg(feature = "gen")]
fn gen_proto() -> Result<()> {
    const PROTO_SRC_FILES: &str = "iscp-proto/proto/**/*.proto";
    const AUTOGEN_DIR: &str = "src/message/proto";

    let proto_files: Vec<_> = glob::glob(PROTO_SRC_FILES)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", PROTO_SRC_FILES, e))
        .filter_map(|res| res.ok())
        .collect();

    if std::path::Path::new(AUTOGEN_DIR).exists() {
        std::fs::remove_dir_all(AUTOGEN_DIR).context("Remove dir")?;
    }
    std::fs::create_dir(AUTOGEN_DIR).context("Create dir")?;

    prost_build::Config::new()
        .out_dir(AUTOGEN_DIR)
        .bytes(["."])
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute("DataID", "#[derive(PartialOrd, Ord, Eq, Hash)]")
        .type_attribute("DataFilter", "#[derive(PartialOrd, Ord, Eq, Hash)]")
        .type_attribute("DataPoint", "#[derive(Eq)]")
        .type_attribute("DownstreamFilter", "#[derive(PartialOrd, Ord, Eq, Hash)]")
        .type_attribute("UpstreamInfo", "#[derive(Eq)]")
        .field_attribute("type", "#[serde(rename = \"type\")]")
        .compile_protos(&proto_files, &[PROTO_SRC_DIR])
        .context("Proto compile")?;

    process_generated_files().context("Process generated files")?;

    Ok(())
}

#[cfg(feature = "gen")]
// Copy proto files to dest_dir and preprocess files
fn process_generated_files() -> Result<()> {
    const PATH: &[&str] = &[
        "src/message/proto/iscp2.v1.rs",
        "src/message/proto/iscp2.v1.extensions.rs",
    ];

    let re = regex::Regex::new("r#type").unwrap();
    for path in PATH {
        let s = std::fs::read_to_string(path)?;
        let replacer = |_caps: &regex::Captures<'_>| "type_";
        let s = re.replace_all(&s, replacer);

        std::fs::write(path, s.as_bytes())?;
    }

    // Workaround for type_attribute
    let re = regex::Regex::new("pub enum DataIdOrAlias").unwrap();
    for path in PATH {
        let s = std::fs::read_to_string(path)?;
        let replacer = |_caps: &regex::Captures<'_>| {
            "#[derive(PartialOrd, Ord, Eq, Hash)]\n    pub enum DataIdOrAlias"
        };
        let s = re.replace_all(&s, replacer);

        std::fs::write(path, s.as_bytes())?;
    }

    Ok(())
}
