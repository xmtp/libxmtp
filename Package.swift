// swift-tools-version: 6.1
import Foundation
import PackageDescription

// The release job records one archive hash for SwiftPM and CocoaPods.
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let localFramework = "sdks/ios/Artifacts/XmtpSdkFFI.xcframework"
let binary: Target
if FileManager.default.fileExists(atPath: root.appendingPathComponent(localFramework).path) {
    binary = .binaryTarget(name: "xmtp_sdkFFI", path: localFramework)
} else {
    let receipt = root.appendingPathComponent("sdks/ios/ReleaseArtifacts.json")
    guard let data = try? Data(contentsOf: receipt),
          let record = try? JSONSerialization.jsonObject(with: data) as? [String: String],
          let url = record["url"], let checksum = record["sha256"]
    else {
        fatalError("Run dev/nix-shell 'just ios build' before building this checkout. A release needs ReleaseArtifacts.json.")
    }
    binary = .binaryTarget(name: "xmtp_sdkFFI", url: url, checksum: checksum)
}

let package = Package(
    name: "XmtpSdk",
    platforms: [.iOS(.v14), .macOS(.v11)],
    products: [.library(name: "XmtpSdk", targets: ["XmtpSdk"])],
    dependencies: [.package(url: "https://github.com/swiftlang/swift-docc-plugin", from: "1.4.0")],
    targets: [
        binary,
        .target(name: "XmtpSdk", dependencies: ["xmtp_sdkFFI"], path: "sdks/ios/Sources/XmtpSdk"),
        .testTarget(name: "XmtpSdkTests", dependencies: ["XmtpSdk"], path: "sdks/ios/Tests/XmtpSdkTests"),
    ],
    swiftLanguageModes: [.v5]
)
