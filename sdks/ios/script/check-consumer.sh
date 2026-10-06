#!/usr/bin/env bash
# Build a consumer of the root XmtpSdk package from Tests/Consumer and run it
# as a bare executable. Then check that each negative consumer in
# Tests/Consumer/Negative fails to compile with the expected diagnostics.
# Run `just ios build` first. The package is staged under target/.
set -euo pipefail

ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
consumer="$ROOT/sdks/ios/Tests/Consumer"
package="$ROOT/target/sdk-ios-consumer"
sources="$package/Sources/Consumer"
# One flag set for every build, so the SDK compiles once. A codec value type
# must be Sendable; strict checking reports it in Swift 5 language mode.
flags=(-Xswiftc -strict-concurrency=complete)

rm -rf "$sources"
mkdir -p "$sources"
cat >"$package/Package.swift" <<EOF
// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "XmtpSdkConsumer",
    platforms: [.macOS(.v15)],
    dependencies: [.package(name: "XmtpSdk", path: "$ROOT")],
    targets: [
        .executableTarget(
            name: "Consumer",
            dependencies: [.product(name: "XmtpSdk", package: "XmtpSdk")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
EOF
cp "$consumer/main.swift" "$sources/main.swift"
swift build --package-path "$package" --product Consumer "${flags[@]}"
"$(swift build --package-path "$package" --show-bin-path "${flags[@]}")/Consumer"

negative="$sources/ConsumerNegative.swift"
log="$(mktemp)"
trap 'rm -f "$negative" "$log"' EXIT
compile_negative() {
  cp "$consumer/Negative/$1" "$negative"
  if swift build --package-path "$package" --product Consumer "${flags[@]}" >"$log" 2>&1; then
    echo "Swift negative consumer $1 compiled" >&2
    exit 1
  fi
}

compile_negative ConsumerNegative.swift
grep -q "type 'NonSendableValue' does not conform to the 'Sendable' protocol" "$log"
grep -q "stored property 'count' of 'Sendable'-conforming class 'MutableCodec' is mutable" "$log"
compile_negative CodecTypeNegative.swift
# The encode, Group and Dm send and prepareMessage, Conversation send, and
# reply calls each reject an Int value: seven distinct source lines (the
# compiler prints each error twice).
test "$(grep -o "ConsumerNegative.swift:[0-9]*:[0-9]*: .*cannot convert value of type 'Int' to expected argument type 'String'" "$log" | cut -d: -f2 | sort -u | wc -l | tr -d ' ')" -eq 7
grep -q "cannot find type 'SDKContentCodec' in scope" "$log"
compile_negative ConsumerTypeNegative.swift
grep -q "cannot convert value of type 'Int' to specified type 'ConversationId'" "$log"
grep -q "cannot convert value of type 'MessageContent' to specified type 'EncodedContent'" "$log"
grep -q "cannot convert value of type 'Conversation' to specified type 'Group'" "$log"
grep -q "cannot convert value of type 'Int' to expected argument type 'MessageId'" "$log"
echo "Swift missing bundle, ID, content, and Group/Dm consumer checks passed"
