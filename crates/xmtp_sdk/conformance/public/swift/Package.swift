// swift-tools-version: 6.1
import PackageDescription

// A separate package that uses only the staged XmtpSdk product. The recipe
// stages that product under target/sdk-public/XmtpSdk.
let package = Package(
    name: "PublicConsumer",
    platforms: [.macOS(.v15)],
    products: [.library(name: "PublicConsumer", targets: ["PublicConsumer"])],
    dependencies: [.package(path: "../../../../../target/sdk-public/XmtpSdk")],
    targets: [
        .target(name: "PublicConsumer", dependencies: [.product(name: "XmtpSdk", package: "XmtpSdk")]),
    ],
    swiftLanguageModes: [.v5]
)
