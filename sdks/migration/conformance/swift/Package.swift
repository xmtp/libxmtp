// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "MigrationConformance", platforms: [.macOS(.v12)],
    dependencies: [.package(path: "../../../../target/migration-packages/ios")],
    targets: [.executableTarget(name: "MigrationConformance", dependencies: [
        .product(name: "XmtpMigration", package: "ios"),
    ])]
)
