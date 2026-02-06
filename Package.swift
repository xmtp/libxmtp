// swift-tools-version: 6.1
// The swift-tools-version declares the minimum version of Swift required to build this package.
//
// NOTE: This file MUST remain at the repository root for Swift Package Manager
// to resolve this package. SPM requires Package.swift at the root of a git
// repository. Do not move it into sdks/ios/ or any subdirectory.

import Foundation
import PackageDescription

let thisPackagePath = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let useLocalBinary = FileManager.default.fileExists(
    atPath: "\(thisPackagePath)/bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework"
)
let useLocalDynamicBinary = FileManager.default.fileExists(
    atPath: "\(thisPackagePath)/bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
)

// Include the dynamic binary target when it exists locally OR for remote consumers.
// SPM downloads ALL declared binary targets (even trait-gated ones), so we must omit the
// dynamic target when only the static xcframework was built locally — otherwise both
// xcframeworks define the same `xmtpv3FFI` clang module, causing a redefinition error.
let includeDynamicTarget = useLocalDynamicBinary || !useLocalBinary

var packageTargets: [Target] = [
    useLocalBinary
        ? .binaryTarget(
            name: "LibXMTPSwiftFFI",
            path: "bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework"
        )
        : .binaryTarget(
            name: "LibXMTPSwiftFFI",
            url:
                "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-29769e9/LibXMTPSwiftFFI.zip",
            checksum: "9ee2ab12f4f57d410276ee5cdff28058564322871fb77cb2169d0f52449b1d6e"
        ),
    .target(
        name: "XMTPiOS",
        dependencies: [
            .product(name: "Connect", package: "connect-swift"),
            .target(name: "LibXMTPSwiftFFI", condition: .when(traits: ["static"])),
            .product(name: "CryptoSwift", package: "CryptoSwift"),
        ]
            + (includeDynamicTarget
                ? [.target(name: "LibXMTPSwiftFFIDynamic", condition: .when(traits: ["dynamic"]))]
                : []),
        path: "sdks/ios/Sources/XMTPiOS"
    ),
    .target(
        name: "XMTPTestHelpers",
        dependencies: ["XMTPiOS"],
        path: "sdks/ios/Sources/XMTPTestHelpers"
    ),
    .testTarget(
        name: "XMTPTests",
        dependencies: ["XMTPiOS", "XMTPTestHelpers"],
        path: "sdks/ios/Tests/XMTPTests"
    ),
]

if includeDynamicTarget {
    packageTargets.insert(
        useLocalDynamicBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-29769e9/LibXMTPSwiftFFIDynamic.zip",
                checksum: "b546484fb9fb4b241ac8148f4945a10962f8f5eec5fcaf4176c31b4f587d5b24"
            ),
        at: 1
    )
}

let package = Package(
    name: "XMTPiOS",
    platforms: [.iOS(.v14), .macOS(.v11)],
    products: [
        .library(
            name: "XMTPiOS",
            targets: ["XMTPiOS"]
        ),
        .library(
            name: "XMTPTestHelpers",
            targets: ["XMTPTestHelpers"]
        ),
    ],
    traits: [
        "static",
        "dynamic",
        .default(enabledTraits: ["static"]),
    ],
    dependencies: [
        .package(url: "https://github.com/bufbuild/connect-swift", exact: "1.2.0"),
        .package(url: "https://github.com/apple/swift-docc-plugin.git", from: "1.4.3"),
        .package(url: "https://github.com/krzyzanowskim/CryptoSwift.git", "1.8.4"..<"2.0.0"),
        .package(url: "https://github.com/SimplyDanny/SwiftLintPlugins", from: "0.62.1"),
    ],
    targets: packageTargets,
    swiftLanguageModes: [.v5]
)
