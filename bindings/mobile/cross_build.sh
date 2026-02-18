#!/bin/bash

RED='\033[0;31m'
NC='\033[0m' # No Color

main() {
  set -ex
  # Dev+Debug is better for debugging Rust crashes, but generates much larger libraries (100's of MB)
  # PROFILE="dev"
  # BINARY_PROFILE="debug"
  PROFILE="release"
  BINARY_PROFILE="release"
  WORKSPACE_MANIFEST="$(cargo locate-project --workspace --message-format=plain)"
  WORKSPACE_PATH="$(dirname $WORKSPACE_MANIFEST)"
  TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')"
  BINDINGS_MANIFEST="$WORKSPACE_PATH/bindings/mobile/Cargo.toml"
  # Go to the workspace root so that the workspace config can be found by cross
  cd $WORKSPACE_PATH
  if ! cross &>/dev/null; then
    echo -e "${RED} 'cargo-cross' not detected. install cargo-cross to continue${NC}";
    exit
  fi
  # Uncomment to build for all targets. aarch64-linux-android is the default target for an emulator on Android Studio on an M1 mac.
  cross build --manifest-path $BINDINGS_MANIFEST --target x86_64-linux-android --target-dir $TARGET_DIR --profile $PROFILE && \
  cross build --manifest-path $BINDINGS_MANIFEST --target i686-linux-android --target-dir $TARGET_DIR --profile $PROFILE && \
  cross build --manifest-path $BINDINGS_MANIFEST --target armv7-linux-androideabi --target-dir $TARGET_DIR --profile $PROFILE && \
  cross build --manifest-path $BINDINGS_MANIFEST --target aarch64-linux-android --target-dir $TARGET_DIR --profile $PROFILE

  # Move everything to jniLibs folder and rename
  LIBRARY_NAME="libxmtpv3"
  TARGET_NAME="libuniffi_xmtpv3"
  cd $(dirname $BINDINGS_MANIFEST) # cd bindings/mobile
  rm -rf jniLibs/
  mkdir -p jniLibs/armeabi-v7a/ && \
      cp ${TARGET_DIR}/armv7-linux-androideabi/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/armeabi-v7a/$TARGET_NAME.so && \
    mkdir -p jniLibs/x86/ && \
      cp ${TARGET_DIR}/i686-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86/$TARGET_NAME.so && \
    mkdir -p jniLibs/x86_64/ && \
      cp ${TARGET_DIR}/x86_64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86_64/$TARGET_NAME.so && \
    mkdir -p jniLibs/arm64-v8a/ && \
      cp ${TARGET_DIR}/aarch64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/arm64-v8a/$TARGET_NAME.so
}

time main
