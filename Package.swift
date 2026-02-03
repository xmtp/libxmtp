// swift-tools-version: 5.6
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
	dependencies: [
		.package(url: "https://github.com/bufbuild/connect-swift", exact: "1.2.0"),
		.package(url: "https://github.com/apple/swift-docc-plugin.git", from: "1.4.3"),
		.package(url: "https://github.com/krzyzanowskim/CryptoSwift.git", "1.8.4" ..< "2.0.0"),
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
				url: "https://github.com/xmtp/libxmtp/releases/download/ios-0.0.0-libxmtp/LibXMTPSwiftFFI.zip",
				checksum: "PLACEHOLDER"
			),
		.target(
			name: "XMTPiOS",
			dependencies: [
				.product(name: "Connect", package: "connect-swift"),
				"LibXMTPSwiftFFI",
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
	]
)
