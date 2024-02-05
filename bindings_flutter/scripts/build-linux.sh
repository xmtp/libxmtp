#!/bin/bash
set -e
cd "$(dirname "$0")"
cd ..

NAME="xmtp_bindings_flutter"
BINDINGS_FLUTTER=$(pwd)
BUILD="$BINDINGS_FLUTTER/platform-build"
ARTIFACTS="$BINDINGS_FLUTTER/artifacts/linux"

rm -rf $BUILD
mkdir -p $BUILD
cd $BUILD
echo
echo "Working in $BUILD"
echo

cargo install cargo-zigbuild

zig_build () {
    local TARGET="$1"
    local PLATFORM_NAME="$2"
    local LIBNAME="$3"
    rustup target add "$TARGET"
    cargo zigbuild --target "$TARGET" -r
    mkdir "$PLATFORM_NAME"
    cp "../target/$TARGET/release/$LIBNAME" "$PLATFORM_NAME/"
}

#zig_build aarch64-unknown-linux-gnu linux-arm64 "lib${NAME}.so"
zig_build x86_64-unknown-linux-gnu linux-x64 "lib${NAME}.so"

echo
echo "Archiving linux binaries"
tar -czvf ${NAME}.linux.tar.gz linux-*
echo
echo "created:"
echo " $BUILD/${NAME}.linux.tar.gz"
echo

echo
echo "Storing artifacts"
rm -rf $ARTIFACTS
mkdir -p $ARTIFACTS
cp -v ${BUILD}/${NAME}.linux.tar.gz ${ARTIFACTS}/${NAME}.linux.tar.gz