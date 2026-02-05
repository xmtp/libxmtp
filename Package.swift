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
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-4f1b2db/LibXMTPSwiftFFI.zip",
                checksum: "9bd14497916b57bc2996fd760248bbfe03423ce2dd13dd155a1da7d88cdfe66c"
            ),
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-4f1b2db/LibXMTPSwiftFFIDynamic.zip",
                checksum: "f31ee060e6fc9291ae1563d84585fea644ac5ec6f489cc9f85405b488e8a6c34"
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
