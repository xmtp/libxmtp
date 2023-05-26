# TODO: Make separate uniffi-bindgen crate
cargo run --features=uniffi/cli \
    --bin uniffi-bindgen \
    generate src/xmtpv3.udl \
    --language kotlin
