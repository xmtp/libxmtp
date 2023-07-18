CRATE_NAME="bindings_ffi"
PROJECT_NAME="xmtpv3"

cargo build --release
pushd .. > /dev/null
rm -f $CRATE_NAME/src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt
bindings_ffi/target/release/ffi-uniffi-bindgen generate \
    --lib-file bindings_ffi/target/release/libbindings_ffi.dylib \
    $CRATE_NAME/src/$PROJECT_NAME.udl \
    --language kotlin
popd > /dev/null

mkdir -p java
cp src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt java/