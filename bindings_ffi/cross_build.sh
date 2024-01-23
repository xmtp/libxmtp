#!/bin/bash

main() {
  set -ex
  # Dev+Debug is better for debugging Rust crashes, but generates much larger libraries (100's of MB)
  # PROFILE="dev"
  # BINARY_PROFILE="debug"
  PROFILE="release"
  BINARY_PROFILE="release"

  # Go to the workspace root so that the workspace config can be found by cross
  cd ..
  # Uncomment to build for all targets. aarch64-linux-android is the default target for an emulator on Android Studio on an M1 mac.
  # cross build --target x86_64-linux-android --target-dir ./target --profile $PROFILE && \
  #     cross build --target i686-linux-android --target-dir ./target --profile $PROFILE && \
  #     cross build --target armv7-linux-androideabi --target-dir ./target --profile $PROFILE && \
  cross build --manifest-path ./bindings_ffi/Cargo.toml --target aarch64-linux-android --target-dir ./bindings_ffi/target --profile $PROFILE

  # Move everything to jniLibs folder and rename
  LIBRARY_NAME="libxmtpv3"
  TARGET_NAME="libuniffi_xmtpv3"
  cd bindings_ffi
  rm -rf jniLibs/
  # mkdir -p jniLibs/armeabi-v7a/ && \
  #     cp target/armv7-linux-androideabi/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/armeabi-v7a/$TARGET_NAME.so && \
  #   mkdir -p jniLibs/x86/ && \
  #     cp target/i686-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86/$TARGET_NAME.so && \
  #   mkdir -p jniLibs/x86_64/ && \
  #     cp target/x86_64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86_64/$TARGET_NAME.so && \
    mkdir -p jniLibs/arm64-v8a/ && \
      cp target/aarch64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/arm64-v8a/$TARGET_NAME.so
}

time main
