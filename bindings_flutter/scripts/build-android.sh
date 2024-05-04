#!/bin/bash
set -e
cd "$(dirname "$0")"
cd ..

NAME="xmtp_bindings_flutter"
BINDINGS_FLUTTER=$(pwd)
BUILD="$BINDINGS_FLUTTER/platform-build"
ARTIFACTS="$BINDINGS_FLUTTER/artifacts/android"

rm -rf $BUILD
mkdir -p $BUILD/jniLibs
cd $BUILD
echo
echo "Working in $BUILD"
echo
cargo install cargo-ndk
rustup target add \
        aarch64-linux-android \
        armv7-linux-androideabi \
        x86_64-linux-android \
        i686-linux-android

echo
echo "Building android binaries"
echo "into ${BUILD}/jniLibs"
cargo ndk -o jniLibs \
        --manifest-path ../Cargo.toml \
        -t armeabi-v7a \
        -t arm64-v8a \
        -t x86 \
        -t x86_64 \
        build --release

echo
echo "Archiving android binaries"
pushd jniLibs
tar -czvf ../$NAME.jniLibs.tar.gz *
popd
echo
echo "created:"
echo " $BUILD/${NAME}.jniLibs.tar.gz"
echo

echo
echo "Storing artifacts"
rm -rf $ARTIFACTS
mkdir -p $ARTIFACTS
cp -v ${BUILD}/${NAME}.jniLibs.tar.gz ${ARTIFACTS}/${NAME}.jniLibs.tar.gz