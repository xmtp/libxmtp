CRATE_NAME="bindings_ffi"
PROJECT_NAME="xmtpv3"

pushd .. > /dev/null
rm -f $CRATE_NAME/src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt
cargo run -p uniffi_bindgen_generator --bin uniffi-bindgen \
    generate $CRATE_NAME/src/$PROJECT_NAME.udl \
    --language kotlin
popd > /dev/null
