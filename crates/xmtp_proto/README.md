# xmtp_proto

This crate generates Rust definitions and methods from the protobuf sources in the workspace root `proto/` directory.

The build script writes generated Rust, serde implementations, and the descriptor set to Cargo `OUT_DIR`.
Run `dev/nix-shell 'buf lint proto'` from the workspace root after you change a protobuf source.
