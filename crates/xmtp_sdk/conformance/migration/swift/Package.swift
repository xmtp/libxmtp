// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "MigrationConformance", platforms: [.macOS(.v15)],
    dependencies: [.package(path: "../../../../../target/sdk-packages/ios")],
    targets: [.executableTarget(name: "MigrationConformance", dependencies: [
        .product(name: "XmtpSdk", package: "ios"),
    ])]
)
