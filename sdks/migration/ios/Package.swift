// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "XmtpMigration",
    platforms: [.macOS(.v12), .iOS(.v15)],
    products: [.library(name: "XmtpMigration", targets: ["XmtpMigration"])],
    targets: [
        .target(name: "XmtpMigration", dependencies: ["XmtpMigrationFFI"], linkerSettings: [
            .linkedFramework("Security"), .linkedFramework("CoreFoundation"), .linkedLibrary("iconv"),
        ]),
        .binaryTarget(name: "XmtpMigrationFFI", path: "XmtpMigrationFFI.xcframework"),
    ]
)
