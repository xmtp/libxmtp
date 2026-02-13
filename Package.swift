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
                "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-4a35627/LibXMTPSwiftFFI.zip",
            checksum: "440d7ad872c63d9a731ad9a4c240ad0e0bc0129bc16eba30a0d9ba06fe7872b3"
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
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-4a35627/LibXMTPSwiftFFIDynamic.zip",
                checksum: "096bcb08e84b9a05676aa4fdbd56e1a5d390d216af045fb0c77d6436836204f2"
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
