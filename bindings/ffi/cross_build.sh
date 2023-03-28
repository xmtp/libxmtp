#!/bin/bash

set -ex

cross build --target x86_64-linux-android --release && \
    cross build --target i686-linux-android --release && \
    cross build --target armv7-linux-androideabi --release && \
    cross build --target aarch64-linux-android --release

# Move everything to jniLibs folder and rename, TODO: should be the same name
LIBRARY_NAME="libcorecrypto_ffi"
TARGET_NAME="libuniffi_corecrypto"
# Change to "release" to generate much smaller libraries
PROFILE="debug"
mkdir -p jniLibs/arm64-v8a/ && \
  cp target/aarch64-linux-android/$PROFILE/$LIBRARY_NAME.so jniLibs/arm64-v8a/$TARGET_NAME.so && \
  mkdir -p jniLibs/armeabi-v7a/ && \
    cp target/armv7-linux-androideabi/$PROFILE/$LIBRARY_NAME.so jniLibs/armeabi-v7a/$TARGET_NAME.so && \
  mkdir -p jniLibs/x86/ && \
    cp target/i686-linux-android/$PROFILE/$LIBRARY_NAME.so jniLibs/x86/$TARGET_NAME.so && \
  mkdir -p jniLibs/x86_64/ && \
    cp target/x86_64-linux-android/$PROFILE/$LIBRARY_NAME.so jniLibs/x86_64/$TARGET_NAME.so
