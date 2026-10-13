// swift-tools-version: 6.2
import PackageDescription

let strict: [SwiftSetting] = [.unsafeFlags(["-warnings-as-errors", "-strict-concurrency=complete"])]
let package = Package(
	name: "ExampleKit",
	platforms: [.iOS(.v26), .macOS(.v15)],
	products: [
		.library(name: "ExampleCore", targets: ["ExampleCore"]),
		.library(name: "ExampleSDK", targets: ["ExampleSDK"]),
	],
	dependencies: [.package(name: "XmtpSdk", path: "../../../..")],
	targets: [
		.target(name: "ExampleCore", swiftSettings: strict),
		.target(
			name: "ExampleSDK",
			dependencies: ["ExampleCore", .product(name: "XmtpSdk", package: "XmtpSdk")],
			swiftSettings: strict,
		),
		.testTarget(name: "ExampleCoreTests", dependencies: ["ExampleCore"], swiftSettings: strict),
		.testTarget(name: "ExampleSDKTests", dependencies: ["ExampleSDK"], swiftSettings: strict),
	],
	swiftLanguageModes: [.v6],
)
