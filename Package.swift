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
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-9d92e9b/LibXMTPSwiftFFI.zip",
                checksum: "b2b8daa6bcfe4f3e9b542c5611faa7318d52c3635168b8caac6e8e67e3b6337c"
            ),
        useLocalBinary
            ? .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                path: "bindings/mobile/build/swift/LibXMTPSwiftFFIDynamic.xcframework"
            )
            : .binaryTarget(
                name: "LibXMTPSwiftFFIDynamic",
                url:
                    "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-9d92e9b/LibXMTPSwiftFFIDynamic.zip",
                checksum: "c6a98c222de63a31ec66b2322eabf8d7db9830ed3dfc2bd078fbb1b0e5969dcd"
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
