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
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-dfd0fb5/LibXMTPSwiftFFI.zip",
                checksum: "976dcd77dab900a7bd0918baa57a9912a14c1cb1109c11c92b055597a4cdddc6"
            ),
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-dfd0fb5/LibXMTPSwiftFFIDynamic.zip",
                checksum: "fa14a32ff9a7f6e7ad9d2c1c9779dd93617fa57a296a915d2f4f34cce5f936b9"
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
