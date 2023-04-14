use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // NOTE: requires googleapis as a submodule, per: https://docs.rs/crate/tonic-build/latest
    // Go to proto folder and run:
    //
    // git submodule add https://github.com/googleapis/googleapis
    // git submodule update --remote

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional") // for older systems
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .file_descriptor_set_path(out_dir.join("store_descriptor.bin"))
        .out_dir("./src")
        .compile(
            &[
                "proto/message_contents/composite.proto",
                "proto/message_contents/signature.proto",
                "proto/message_contents/ciphertext.proto",
                "proto/message_contents/public_key.proto",
                "proto/message_contents/contact.proto",
                "proto/message_contents/invitation.proto",
                "proto/message_contents/private_key.proto",
                "proto/message_contents/message.proto",
                "proto/message_contents/content.proto",
                "proto/message_api/v1/authn.proto",
                "proto/message_api/v1/message_api.proto",
                "proto/keystore_api/v1/keystore.proto",
            ],
            &["proto", "proto/googleapis"],
        )?;

    Ok(())
}
