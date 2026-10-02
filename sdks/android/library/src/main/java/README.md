# Generated Kotlin API

The public package is `uniffi.xmtp_sdk`. `sdks/android/dev/bindings` stages matched
generated sources and `libxmtp_sdk.so` from the new Android Nix outputs. Run it
through `dev/nix-shell` from the repository root. See the SDK [rules](../../../../AGENTS.md)
for commands and the [guide](../../../../README.md) for public API use.

The maintained main Kotlin files add Context storage and process lifecycle
control. Rust owns all standard codecs, cryptography, storage, and transfer
state. Do not add a wrapper over the old mobile bindings.
