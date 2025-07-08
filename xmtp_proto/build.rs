fn main() {
    use std::{env, path::PathBuf};

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Generate descriptor sets individually
    let descriptor_sets = [
        ("identity_api_v1", "proto/identity/api/v1/identity.proto"),
        ("mls_api_v1", "proto/mls/api/v1/mls.proto"),
        ("message_api", "proto/xmtpv4/message_api/message_api.proto"),
        ("payer_api", "proto/xmtpv4/payer_api/payer_api.proto"),
    ];

    for (name, proto) in &descriptor_sets {
        let path = out_dir.join(format!("{name}.bin"));

        tonic_build::configure()
            .file_descriptor_set_path(&path)
            .build_server(true)
            .build_client(true)
            .out_dir("src/gen")
            .compile_well_known_types(true)
            .extern_path(".google.protobuf", "::pbjson_types")
            .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
            .emit_rerun_if_changed(true)
            .compile_protos(
                &[*proto],
                &[
                    "proto",
                    "third_party/googleapis",
                    "third_party/grpc-gateway/third_party/googleapis",
                    "third_party/grpc-gateway",
                ],
            )
            .unwrap_or_else(|e| panic!("Failed to compile {proto}: {e}"));
    }

    // Full codegen pass without descriptor set (optional)
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/gen")
        .compile_well_known_types(true)
        .extern_path(".google.protobuf", "::pbjson_types")
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .emit_rerun_if_changed(true)
        .compile_protos(
            &[
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
            ],
            &[
                "proto",
                "third_party/googleapis",
                "third_party/grpc-gateway/third_party/googleapis",
                "third_party/grpc-gateway",
            ],
        )
        .expect("Failed to compile full proto set");

    println!("cargo:rerun-if-changed=build.rs");
}
