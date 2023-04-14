#!/bin/bash

set -ex

# This script copies the built XCFramework to a the xmtp_rust_swift repo, which is a Swift package
# that encapsulates all of the good stuff here.

# Look for an xmtp_rust_swift repo at the sibling layer of the top level of this repo so ../../../

REPONAME="xmtp_rust_swift"
REPOPATH="../../../$REPONAME"
# Now move the XMTPRustSwift.xcframework to the Swift package
rm -rf "$REPOPATH/XMTPRustSwift.xcframework"
cp -R "XMTPRustSwift.xcframework" "$REPOPATH"

# Need to copy any Swift file in ./include/Generated to $REPOPATH/Sources/XMTPRust/*
FILES=$(find ./include/Generated -name "*.swift")
# HACK HACK HACK
# Here's the ultra-hack, we need to inject "import XMTPRustSwift" into the top of the Swift files
# before copying them over so they'll get the headers we moved to "include/Generated".
for f in $FILES
do
    echo "Injecting 'import XMTPRustSwift' text as first line into $f"
    sed -i '' '1s/^/import XMTPRustSwift\n/' "$f"
    echo "Copying $f"
    mv "$f" "$REPOPATH/Sources/XMTPRust/"
done
