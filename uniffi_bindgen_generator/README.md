# Uniffi Bindings Generator

This is a binary crate used to generate uniffi bindings, following the instructions in https://mozilla.github.io/uniffi-rs/tutorial/foreign_language_bindings.html#multi-crate-workspaces.

The actual usage of this crate can be found in build scripts in the other crates.

Example usage:

```
cargo run -p uniffi_bindgen_generator --bin uniffi-bindgen \
    generate xmtp_dh/src/xmtp_dh.udl \
    --language kotlin
```
