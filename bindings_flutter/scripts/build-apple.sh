#!/bin/bash
set -e
cd "$(dirname "$0")"
cd ..

# First make some space to work
NAME="xmtp_bindings_flutter"
BINDINGS_FLUTTER=$(pwd)
BUILD="$BINDINGS_FLUTTER/platform-build"
LIB="lib$NAME.a"
ARTIFACTS="$BINDINGS_FLUTTER/artifacts/apple"

rm -rf $BUILD
mkdir $BUILD
cd $BUILD
echo
echo "Working in $BUILD"
echo
# Then build each target
for TARGET in \
        aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim \
        aarch64-apple-darwin x86_64-apple-darwin
do
    echo
    echo "Building $TARGET"
    rustup target add $TARGET
    cargo build -r --target=$TARGET
done

# Prepare the single dylib for use in tests
echo
echo "Mergin the test shared dylib"
echo
lipo -create -output "${BUILD}/lib$NAME.dylib" \
        "${BINDINGS_FLUTTER}/target/aarch64-apple-darwin/release/lib${NAME}.dylib" \
        "${BINDINGS_FLUTTER}/target/x86_64-apple-darwin/release/lib${NAME}.dylib"

# Prepare the binaries for use in the simulator and XCFramework
echo
echo "Merging iOS Simulator binary"
echo
mkdir $BUILD/ios-sim-lipo
IOS_SIM_LIPO=$BUILD/ios-sim-lipo/$LIB
lipo -create -output $IOS_SIM_LIPO \
        $BINDINGS_FLUTTER/target/aarch64-apple-ios-sim/release/$LIB \
        $BINDINGS_FLUTTER/target/x86_64-apple-ios/release/$LIB

echo
echo "Merging macOS binary"
echo
mkdir $BUILD/mac-lipo
MAC_LIPO=$BUILD/mac-lipo/$LIB
lipo -create -output $MAC_LIPO \
        $BINDINGS_FLUTTER/target/aarch64-apple-darwin/release/$LIB \
        $BINDINGS_FLUTTER/target/x86_64-apple-darwin/release/$LIB

echo "Creating XCFramework"
echo
xcodebuild -quiet -create-xcframework \
        -library $IOS_SIM_LIPO \
        -library $MAC_LIPO \
        -library $BINDINGS_FLUTTER/target/aarch64-apple-ios/release/$LIB \
        -output $BUILD/$NAME.xcframework

echo
echo "Archiving XCFramework"
pushd $BUILD
zip -q -r $NAME.xcframework.zip $NAME.xcframework
popd
echo
echo "created:"
echo " $BUILD/$NAME.xcframework.zip"
echo

echo
echo "Moving artifacts"
rm -rf $ARTIFACTS
mkdir -p $ARTIFACTS
cp -v $BUILD/$NAME.xcframework.zip $ARTIFACTS/$NAME.xcframework.zip
cp -v $BUILD/lib$NAME.dylib $ARTIFACTS/lib$NAME.dylib