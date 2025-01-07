// swift-tools-version: 5.6
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

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
		.package(url: "https://github.com/tesseract-one/CSecp256k1.swift.git", from: "0.2.0"),
		.package(url: "https://github.com/bufbuild/connect-swift", exact: "1.0.0"),
		.package(url: "https://github.com/apple/swift-docc-plugin.git", from: "1.4.3"),
		.package(url: "https://github.com/krzyzanowskim/CryptoSwift.git", exact: "1.8.3"),
		.package(url: "https://github.com/xmtp/libxmtp-swift.git", exact: "3.0.18")
	],
	targets: [
		.target(
			name: "XMTPiOS",
			dependencies: [
				.product(name: "CSecp256k1", package: "CSecp256k1.swift"),
				.product(name: "Connect", package: "connect-swift"),
				.product(name: "LibXMTP", package: "libxmtp-swift"),
				.product(name: "CryptoSwift", package: "CryptoSwift"),
			]
		),
		.target(
			name: "XMTPTestHelpers",
			dependencies: ["XMTPiOS"]
		),
		.testTarget(
			name: "XMTPTests",
			dependencies: ["XMTPiOS", "XMTPTestHelpers"]
		),
	]
)
