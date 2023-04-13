#!/usr/bin/env bash
# This script copies the built XCFramework to all DerivedData directories
# that contain a Package.swift file that references the XCFramework.
# This is needed in tandem with xmtp-rust-swift development builds that use a local xcframework .binaryTarget
#
# Steps:
# - Scan derived data for xmtp-rust-swift checkouts
# - Look for paths like: /Users/michaelx/Library/Developer/Xcode/DerivedData/ios2-elqrwwtgpigvwehajufcujbhyfpg/SourcePackages/checkouts/xmtp-rust-swift/
# - Then copy the XMTPRustSwift.xcframework to the checkout

derived_data_dirs=$(find ~/Library/Developer/Xcode/DerivedData -type d -name "xmtp-rust-swift")
for dir in $derived_data_dirs; do
  if grep -q "XMTPRustSwift.xcframework" "$dir/Package.swift"; then
    echo "Copying to $dir"
    cp -r XMTPRustSwift.xcframework "$dir"
  else
    echo "Skipping $dir"
  fi
done
