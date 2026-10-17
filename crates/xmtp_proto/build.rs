use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use tonic_prost_build::{Builder, Config, configure};
use walkdir::WalkDir;

const SERVER_CFG: &str =
    r#"#[cfg(any(not(target_arch = "wasm32"), feature = "grpc_server_impls"))]"#;

fn codegen_configure(mut builder: Builder) -> Builder {
    for package in ["xmtp.backend.v1", "xmtp.identity.api.v1"] {
        builder = builder.server_mod_attribute(package, SERVER_CFG);
    }
    builder
}

fn proto_files(proto_root: &Path) -> Result<Vec<PathBuf>, walkdir::Error> {
    let entries = WalkDir::new(proto_root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|entry| {
            entry.file_type().is_dir() || entry.path().extension() == Some(OsStr::new("proto"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut files = entries
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn merge_serde_files(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let entries = WalkDir::new(out_dir)
        .max_depth(1)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut serde_files = entries
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(".serde.rs"))
        })
        .collect::<Vec<_>>();
    serde_files.sort();

    for serde_path in serde_files {
        let file_name = serde_path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or("generated serde path is not valid UTF-8")?;
        let rust_path = out_dir.join(file_name.replace(".serde.rs", ".rs"));
        let serde_source = fs::read(&serde_path)?;
        let mut rust_file = OpenOptions::new().append(true).open(&rust_path)?;
        rust_file.write_all(b"\n")?;
        rust_file.write_all(&serde_source)?;
        fs::remove_file(serde_path)?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let workspace_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or("xmtp_proto must be inside the workspace crates directory")?;
    let proto_root = workspace_dir.join("proto");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    println!("cargo:rerun-if-changed={}", proto_root.display());
    println!("cargo:rerun-if-changed=build.rs");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir)?;
    }
    fs::create_dir_all(&out_dir)?;

    let files = proto_files(&proto_root)?;
    let descriptor_path = out_dir.join("proto_descriptor.bin");

    let mut config = Config::new();
    config.enable_type_names().include_file("mod.rs");

    codegen_configure(
        configure()
            .compile_well_known_types(true)
            .protoc_arg("--experimental_allow_proto3_optional")
            .out_dir(&out_dir)
            .extern_path(".google.protobuf", "::pbjson_types")
            .file_descriptor_set_path(&descriptor_path)
            .build_transport(false)
            .build_client(cfg!(feature = "grpc_client_impls")),
    )
    .compile_with_config(config, &files, &[proto_root])?;

    let descriptors = fs::read(&descriptor_path)?;
    pbjson_build::Builder::new()
        .out_dir(&out_dir)
        .register_descriptors(&descriptors)?
        .ignore_unknown_fields()
        .preserve_proto_field_names()
        .build(&[".xmtp"])?;
    merge_serde_files(&out_dir)?;

    Ok(())
}
