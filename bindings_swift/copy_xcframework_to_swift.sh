#!/bin/bash

set -ex

# This script copies the built XCFramework to a the xmtp_rust_swift repo, which is a Swift package
# that encapsulates all of the good stuff here.

# Look for an xmtp_rust_swift repo at the sibling layer of the top level of this repo so ../../

REPONAME="xmtp-rust-swift"
REPOPATH="../../$REPONAME"
# Now move the XMTPRustSwift.xcframework to the Swift package
rm -rf "$REPOPATH/XMTPRustSwift.xcframework"
cp -R "XMTPRustSwift.xcframework" "$REPOPATH"

# Need to copy any Swift file in ./include/Generated to $REPOPATH/Sources/XMTPRust/*
FILES=$(find ./include/Generated -name "*.swift")

# HACK HACK HACK
# Here's the ultra-hack, we need to inject "import XMTPRustSwift" into the top of the Swift files
# before copying them over so they'll get the headers we moved to "include/Generated".
#
# Moreover, we inject the following lines to allow RustStrings to be interpreted as NSErrors and automatically
# displayed in the debugger:
# https://github.com/chinedufn/swift-bridge/issues/150
#
# extension RustString: @unchecked Sendable {}
#
# extension RustString: LocalizedError {
#     public var errorDescription: String? {
#         return NSLocalizedString("XMTP Rust Error: \(self.as_str().toString())", comment: self.as_str().toString())
#     }
# }
add_xmtprustswift_import() {
    echo "Injecting 'import XMTPRustSwift' text as first line into $1"
    sed -i '' '1s/^/import XMTPRustSwift\n\n/' "$1"
}
add_foundation_import() {
    echo "Injecting 'import Foundation' text as first line into $1"
    sed -i '' '1s/^/import Foundation\n\n/' "$1"
}
add_nserror_helpers() {
    sed -i '' '1s/^/extension RustString: @unchecked Sendable {}\nextension RustString: LocalizedError {\n    public var errorDescription: String? {\n        return NSLocalizedString("XMTP Rust Error: \\(self.as_str().toString())", comment: self.as_str().toString())\n    }\n}\n\n/' "$1"
}
for f in $FILES
do
    # If it's a xmtp_rust_swift.swift file, then do the injection
    if [[ $f == *"xmtp_rust_swift.swift" ]]; then
      add_nserror_helpers "$f"
      # Only the xmtp_rust_swift.swift file needs Foundation
      add_foundation_import "$f"
    fi
    # All files need "import XMTPRustSwift"
    add_xmtprustswift_import "$f"
    echo "Copying $f"
    mv "$f" "$REPOPATH/Sources/XMTPRust/"
done

