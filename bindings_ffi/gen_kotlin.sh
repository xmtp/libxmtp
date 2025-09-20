PROJECT_NAME="xmtpv3"

RED='\033[0;31m'
NC='\033[0m' # No Color

WORKSPACE_MANIFEST="$(cargo locate-project --workspace --message-format=plain)"
WORKSPACE_PATH="$(dirname $WORKSPACE_MANIFEST)"
BINDINGS_MANIFEST="$WORKSPACE_PATH/bindings_ffi/Cargo.toml"
BINDINGS_PATH="$(dirname $BINDINGS_MANIFEST)"
TARGET_DIR="$WORKSPACE_PATH/target"
XMTP_ANDROID="${1:-$(realpath ../../xmtp-android)}"

if [ ! -d $XMTP_ANDROID ]; then
  echo "${RED}xmtp-android directory not detected${NC}"
  echo "${RED}Ensure \`github.com/xmtp/xmtp_android\` is cloned as a sibling directory or passed as the first argument to this script.${NC}"
  exit
fi
echo "Android Directory: $XMTP_ANDROID"

cd $WORKSPACE_PATH
cargo build --release -p xmtpv3
rm -f $BINDINGS_PATH/src/uniffi/$PROJECT_NAME/$PROJECT_NAME.kt
cargo run --bin ffi-uniffi-bindgen \
  --manifest-path $BINDINGS_MANIFEST \
  --features uniffi/cli --release -- \
  generate \
  --lib-file $TARGET_DIR/release/lib$PROJECT_NAME.dylib $BINDINGS_PATH/src/$PROJECT_NAME.udl \
  --language kotlin

cd $BINDINGS_PATH
make libxmtp-version
cp libxmtp-version.txt src/uniffi/$PROJECT_NAME/

# 1) Replace `return "xmtpv3"` with `return "uniffi_xmtpv3"`
# 2) Replace `value.forEach { (k, v) ->` with `value.iterator().forEach { (k, v) ->`
#    in the file xmtpv3.kt
sed -i '' \
    -e 's/return "xmtpv3"/return "uniffi_xmtpv3"/' \
    -e 's/value\.forEach { (k, v) ->/value.iterator().forEach { (k, v) ->/g' \
    "$BINDINGS_PATH/src/uniffi/xmtpv3/xmtpv3.kt"

echo "Replacements done in $XMTP_ANDROID/library/src/main/java/xmtpv3.kt"

cp $BINDINGS_PATH/src/uniffi/xmtpv3/xmtpv3.kt $XMTP_ANDROID/library/src/main/java/xmtpv3.kt
cp $BINDINGS_PATH/src/uniffi/xmtpv3/libxmtp-version.txt $XMTP_ANDROID/library/src/main/java/libxmtp-version.txt

# Read the version number from libxmtp-version file
VERSION=$(head -n 1 libxmtp-version.txt | cut -d' ' -f2 | cut -c1-7)

# Get the crate version from bindings_ffi Cargo.toml using cargo metadata
echo "BINDINGS_MANIFEST for crate version command: $BINDINGS_MANIFEST"
CRATE_VERSION=$(cargo metadata --manifest-path $BINDINGS_MANIFEST --format-version 1 | 
                jq -r '.packages[] | select(.name == "xmtpv3") | .version')
echo "CRATE_VERSION: $CRATE_VERSION"

# Construct the download URL using both versions
DOWNLOAD_URL="https://github.com/xmtp/libxmtp/releases/download/kotlin-bindings-${CRATE_VERSION}.${VERSION}/LibXMTPKotlinFFI.zip"
echo "DOWNLOAD_URL: $DOWNLOAD_URL"

# Remove existing zip file if it exists
rm -f src/uniffi/$PROJECT_NAME/LibXMTPKotlinFFI.zip
rm -rf src/uniffi/$PROJECT_NAME/jniLibs

# Download the zip file (now with 16 KB aligned libraries from GitHub Actions)
echo "Downloading from: ${DOWNLOAD_URL}"
curl -fL -o ./LibXMTPKotlinFFI.zip "${DOWNLOAD_URL}"

if [ $? -eq 0 ]; then
    echo "Successfully downloaded LibXMTPKotlinFFI.zip (16 KB aligned)"
else
    echo "Failed to download zip file. Make sure the kotlin bindings GH action for this commit is finished: https://github.com/xmtp/libxmtp/actions/workflows/release-kotlin-bindings.yml"
    exit 1
fi

mv ./LibXMTPKotlinFFI.zip src/uniffi/$PROJECT_NAME/
cd src/uniffi/$PROJECT_NAME/
unzip -o LibXMTPKotlinFFI.zip
cd ../../..

cp -r $BINDINGS_PATH/src/uniffi/$PROJECT_NAME/jniLibs/* $XMTP_ANDROID/library/src/main/jniLibs
