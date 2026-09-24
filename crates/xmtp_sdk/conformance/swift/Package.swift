// swift-tools-version: 6.1
import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let workspace = (0 ..< 4).reduce(packageRoot) { path, _ in path.deletingLastPathComponent() }
let staticLibrary = workspace.appendingPathComponent("target/debug/libxmtp_sdk.a").path
let opensslLibrary = ProcessInfo.processInfo.environment["SDK_OPENSSL_LIB_DIR"] ?? ""

let package = Package(
    name: "XmtpSdkConformance",
    platforms: [.macOS(.v15)],
    products: [.executable(name: "XmtpSdkConformance", targets: ["Conformance"])],
    targets: [
        .systemLibrary(name: "xmtp_sdkFFI", path: "Sources/xmtp_sdkFFI"),
        .target(
            name: "XmtpSdk",
            dependencies: ["xmtp_sdkFFI"],
            path: "Sources/XmtpSdk",
            linkerSettings: [.unsafeFlags([staticLibrary, "-L", opensslLibrary, "-lcrypto", "-lssl"])]
        ),
        .executableTarget(name: "Conformance", dependencies: ["XmtpSdk"]),
    ],
    swiftLanguageModes: [.v5]
)
