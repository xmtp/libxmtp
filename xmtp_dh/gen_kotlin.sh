pushd .. > /dev/null
rm -f xmtp_dh/src/uniffi/xmtp_dh/xmtp_dh.kt
cargo run -p uniffi_bindgen_generator --bin uniffi-bindgen \
    generate xmtp_dh/src/xmtp_dh.udl \
    --language kotlin \
    --lib-file xmtp_dh/target/release/libxmtp_dh.dylib
popd > /dev/null
