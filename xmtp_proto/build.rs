use color_eyre::eyre::Result;
use relative_path::PathExt;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use tonic_prost_build::{configure, Builder, Config};
use walkdir::WalkDir;
use xshell::{cmd, Shell};

fn codegen_configure(builder: Builder) -> Builder {
    builder
        .build_transport(false)
        .server_mod_attribute(
            "xmtp.identity.api.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .server_mod_attribute(
            "xmtp.mls_validation.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .server_mod_attribute(
            "xmtp.message_api.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .server_mod_attribute("xmtp.mls.api.v1", r#"#[cfg(not(target_arch = "wasm32"))]"#)
        .server_mod_attribute("xmtp.xmtpv4", r#"#[cfg(not(target_arch = "wasm32"))]"#)
        .server_mod_attribute(
            "xmtp.xmtpv4.payer_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .server_mod_attribute(
            "xmtp.xmtpv4.message_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .server_mod_attribute(
            "xmtp.xmtpv4.metadata_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
}

fn clone_proto_repos(out_dir: &PathBuf, git_ref: &str) -> Result<()> {
    let sh = Shell::new()?;
    if !std::fs::exists(out_dir.join("grpc-gateway"))? {
        cmd!(
            sh,
            "git clone https://github.com/grpc-ecosystem/grpc-gateway.git {out_dir}/grpc-gateway"
        )
        .run()?;
    }
    if !std::fs::exists(out_dir.join("googleapis"))? {
        cmd!(
            sh,
            "git clone https://github.com/googleapis/googleapis.git {out_dir}/googleapis"
        )
        .run()?;
    }
    if std::fs::exists(out_dir.join("proto"))? {
        std::fs::remove_dir_all(out_dir.join("proto"))?;
    }
    cmd!(
        sh,
        "git clone https://github.com/xmtp/proto.git --revision {git_ref} {out_dir}/proto"
    )
    .run()?;
    Ok(())
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto_version");
    println!("cargo:rerun-if-env-changed=GEN_PROTOS");
    let update = std::env::var("GEN_PROTOS");
    let should_update = matches!(update, Ok(s) if s == "true" || s == "1");
    if !should_update {
        return Ok(());
    }

    if !cmd_exists("protoc") {
        panic!("xmtp_proto buildscript requires protoc on $PATH");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    println!("out_dir = {}", out_dir.display());
    let revision = std::fs::read_to_string(manifest.join("proto_version"))?;
    clone_proto_repos(&out_dir, revision.trim())?;

    let include_paths = &[
        &format!("{}/proto/proto", out_dir.display()),
        &format!("{}/grpc-gateway/", out_dir.display()),
        &format!("{}/grpc-gateway/third_party/googleapis/", out_dir.display()),
        &format!("{}/googleapis/", out_dir.display()),
    ];

    let proto_files = WalkDir::new(out_dir.join("proto").join("proto"))
        .min_depth(1)
        .into_iter()
        .filter_entry(|f| {
            f.path().extension() == Some(OsStr::new("proto")) || f.file_type().is_dir()
        })
        .filter_map(|f| {
            let p = f.unwrap().into_path();
            if p.is_dir() {
                None
            } else {
                Some(p)
            }
        })
        .collect::<Vec<_>>();

    let files = &proto_files
        .iter()
        .map(|p| p.relative_to(out_dir.join("proto").join("proto")).unwrap())
        .map(|p| p.to_string())
        .collect::<Vec<String>>();
    for file in files {
        println!("{}", file);
    }

    let descriptor_path = manifest.join("src/gen/proto_descriptor.bin");

    let files = files.iter().map(|s| s.as_ref()).collect::<Vec<&str>>();
    let includes = include_paths
        .iter()
        .map(|s| s.as_ref())
        .collect::<Vec<&str>>();

    let mut config = Config::new();
    config.enable_type_names();
    let builder = configure()
        .compile_well_known_types(true)
        .out_dir("src/gen")
        .extern_path(".google.protobuf", "::pbjson_types")
        .file_descriptor_set_path(&descriptor_path);

    let builder = codegen_configure(builder);
    builder
        // include can be used to generate the mod.rs file, before
        // editing it to include serde additions
        // .include_file("mod.rs")
        .compile_with_config(config, &files, &includes)
        .expect("Failed to compile protos");
    let descriptors = std::fs::read(&descriptor_path)?;
    pbjson_build::Builder::new()
        .out_dir("src/gen")
        .register_descriptors(&descriptors)?
        .ignore_unknown_fields()
        .preserve_proto_field_names()
        .build(&[".xmtp"])?;

    Ok(())
}

fn cmd_exists(program: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for p in path.split(":") {
            let p_str = format!("{}/{}", p, program);
            if std::fs::metadata(p_str).is_ok() {
                return true;
            }
        }
    }
    false
}
