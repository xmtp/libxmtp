use std::env;
use std::path::PathBuf;

fn codegen_configure(builder: tonic_build::Builder) -> tonic_build::Builder {
    builder
        .client_mod_attribute(
            "xmtp.identity.api.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .client_mod_attribute(
            "xmtp.message_api.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .client_mod_attribute("xmtp.mls.api.v1", r#"#[cfg(not(target_arch = "wasm32"))]"#)
        .client_mod_attribute(
            "xmtp.mls_validation.v1",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .client_mod_attribute("xmtp.xmtpv4", r#"#[cfg(not(target_arch = "wasm32"))]"#)
        .client_mod_attribute(
            "xmtp.xmtpv4.payer_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .client_mod_attribute(
            "xmtp.xmtpv4.message_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
        .client_mod_attribute(
            "xmtp.xmtpv4.metadata_api",
            r#"#[cfg(not(target_arch = "wasm32"))]"#,
        )
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

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let descriptor_path = out_dir.join("descriptor.bin");

    // Generate specific descriptor sets for targeted APIs
    let descriptor_targets = [
        ("identity_api_v1", "proto/identity/api/v1/identity.proto"),
        ("mls_api_v1", "proto/mls/api/v1/mls.proto"),
        ("message_api", "proto/xmtpv4/message_api/message_api.proto"),
        ("payer_api", "proto/xmtpv4/payer_api/payer_api.proto"),
    ];

    let include_paths = &[
        "proto",
        "third_party/googleapis",
        "third_party/grpc-gateway/third_party/googleapis",
        "third_party/grpc-gateway",
    ];

    for (name, proto) in descriptor_targets {
        let descriptor_path = out_dir.join(format!("{name}.bin"));

        let mut config = prost_build::Config::new();
        config
            .enable_type_names()
            .file_descriptor_set_path(&descriptor_path)
            .compile_well_known_types()
            .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
            .extern_path(".google.protobuf", "::pbjson_types");

        // Compile with tonic
        let builder = tonic_build::configure();
        let builder = codegen_configure(builder);
        builder
            .out_dir("src/gen") // optional: customize output dir
            .compile_protos_with_config(config, &[proto], include_paths)
            .unwrap_or_else(|e| panic!("Failed to generate descriptor set for {proto}: {e}"));

        println!("cargo:rerun-if-changed={proto}");
    }

    // List of proto files to compile
    let proto_files = &[
        "proto/device_sync/device_sync.proto",
        "proto/device_sync/consent_backup.proto",
        "proto/device_sync/content.proto",
        "proto/device_sync/event_backup.proto",
        "proto/device_sync/group_backup.proto",
        "proto/device_sync/message_backup.proto",
        "proto/identity/api/v1/identity.proto",
        "proto/identity/credential.proto",
        "proto/identity/associations/association.proto",
        "proto/identity/associations/signature.proto",
        "proto/keystore_api/v1/keystore.proto",
        "proto/message_api/v1/message_api.proto",
        "proto/message_contents/message.proto",
        "proto/mls/message_contents/group_membership.proto",
        "proto/mls/message_contents/group_metadata.proto",
        "proto/mls/message_contents/content.proto",
        "proto/mls/message_contents/group_mutable_metadata.proto",
        "proto/mls/message_contents/group_permissions.proto",
        "proto/mls/message_contents/out_of_band.proto",
        "proto/mls/message_contents/wrapper_encryption.proto",
        "proto/mls/message_contents/transcript_messages.proto",
        "proto/mls/api/v1/mls.proto",
        "proto/mls/database/intents.proto",
        "proto/mls/message_contents/content_types/reaction.proto",
        "proto/mls/message_contents/content_types/multi_remote_attachment.proto",
        "proto/mls/message_contents/content_types/wallet_send_calls.proto",
        "proto/mls_validation/v1/service.proto",
        "proto/xmtpv4/envelopes/envelopes.proto",
        "proto/xmtpv4/envelopes/payer_report.proto",
        "proto/xmtpv4/message_api/message_api.proto",
        "proto/xmtpv4/message_api/misbehavior_api.proto",
        "proto/xmtpv4/metadata_api/metadata_api.proto",
        "proto/xmtpv4/payer_api/payer_api.proto",
    ];

    // Configure prost
    let mut config = prost_build::Config::new();
    config
        .enable_type_names()
        .file_descriptor_set_path(&descriptor_path)
        .compile_well_known_types()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .extern_path(".google.protobuf", "::pbjson_types");

    // Compile with tonic
    let builder = tonic_build::configure();
    let builder = codegen_configure(builder);
    builder
        .out_dir("src/gen") // optional: customize output dir
        .compile_protos_with_config(config, proto_files, include_paths)
        .expect("Failed to compile protos");

    println!("cargo:rerun-if-changed=build.rs");
    for proto in proto_files {
        println!("cargo:rerun-if-changed={}", proto);
    }
}
