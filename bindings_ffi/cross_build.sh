#!/bin/bash

main() {
  set -ex

  # Change to "release" to generate much smaller libraries
  PROFILE="dev"
  BINARY_PROFILE="debug"

  cross build --target x86_64-linux-android --target-dir ./target --profile $PROFILE && \
      cross build --target i686-linux-android --target-dir ./target --profile $PROFILE && \
      cross build --target armv7-linux-androideabi --target-dir ./target --profile $PROFILE && \
      cross build --target aarch64-linux-android --target-dir ./target --profile $PROFILE

  # Move everything to jniLibs folder and rename, TODO: should be the same name
  LIBRARY_NAME="libbindings_ffi"
  TARGET_NAME="libuniffi_xmtpv3"
  rm -rf jniLibs/
  mkdir -p jniLibs/arm64-v8a/ && \
    cp target/aarch64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/arm64-v8a/$TARGET_NAME.so && \
    mkdir -p jniLibs/armeabi-v7a/ && \
      cp target/armv7-linux-androideabi/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/armeabi-v7a/$TARGET_NAME.so && \
    mkdir -p jniLibs/x86/ && \
      cp target/i686-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86/$TARGET_NAME.so && \
    mkdir -p jniLibs/x86_64/ && \
      cp target/x86_64-linux-android/$BINARY_PROFILE/$LIBRARY_NAME.so jniLibs/x86_64/$TARGET_NAME.so
}

time main
