cargo build --release
pushd .. > /dev/null
rm -f xmtp_dh/src/uniffi/xmtp_dh/xmtp_dh.kt
xmtp_dh/target/release/xmtp-dh-gen generate \
    --lib-file xmtp_dh/target/release/libxmtp_dh.dylib \
    xmtp_dh/src/xmtp_dh.udl \
    --language kotlin
popd > /dev/null
