CRATE_NAME="bindings_ffi"
PROJECT_NAME="xmtpv3"

cargo build --release
pushd .. > /dev/null
rm -f $CRATE_NAME/src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt
bindings_ffi/target/release/ffi-uniffi-bindgen generate \
    --lib-file bindings_ffi/target/release/libxmtpv3.dylib \
    $CRATE_NAME/src/$PROJECT_NAME.udl \
    --language kotlin
popd > /dev/null
make libxmtp-version
mv libxmtp-version.txt src/uniffi/$PROJECT_NAME/
