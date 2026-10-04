// swift-tools-version: 6.1
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let workspace = (0 ..< 4).reduce(packageRoot) { path, _ in path.deletingLastPathComponent() }

/// Conformance links the root Apple package.
let package = Package(
    name: "XmtpSdkConformance",
    platforms: [.macOS(.v15)],
    products: [.executable(name: "XmtpSdkConformance", targets: ["Conformance"])],
    dependencies: [.package(name: "XmtpSdk", path: workspace.path)],
    targets: [
        .executableTarget(
            name: "Conformance",
            dependencies: [.product(name: "XmtpSdk", package: "XmtpSdk")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
