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
    targets: [
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFI",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFI.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFI",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-88ddfad/LibXMTPSwiftFFI.zip",
                checksum: "581c37de997498d32eb1eeb5f62297b381787e4857ba80cc99907dfaba8bf878"
            ),
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-88ddfad/LibXMTPSwiftFFIDynamic.zip",
                checksum: "f0ea01352b579241efacd3c38e93130ff2b21f4a908fc56faee4f3c5b7d25d62"
            ),
        .target(
            name: "XMTPiOS",
            dependencies: [
                .product(name: "Connect", package: "connect-swift"),
                .target(name: "LibXMTPSwiftFFI", condition: .when(traits: ["static"])),
                .target(name: "LibXMTPSwiftFFIDynamic", condition: .when(traits: ["dynamic"])),
                .product(name: "CryptoSwift", package: "CryptoSwift"),
            ],
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
    ],
    swiftLanguageModes: [.v5]
)
