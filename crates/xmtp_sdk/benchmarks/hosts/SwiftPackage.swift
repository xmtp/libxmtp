// swift-tools-version: 5.9
import Foundation
import PackageDescription

let environment = ProcessInfo.processInfo.environment
let side = environment["BENCHMARK_SIDE"]!
let sdkPath = environment["BENCHMARK_SDK_PACKAGE"]!
precondition(side == "old" || side == "new")
let product = side == "old" ? "XMTPiOS" : "XmtpSdk"
let identity = URL(fileURLWithPath: sdkPath).lastPathComponent.lowercased()
let package = Package(
    name: "XmtpBenchmark", platforms: [.macOS(.v13)],
    dependencies: [.package(path: sdkPath)],
    targets: [.executableTarget(name: "XmtpBenchmark",
                                dependencies: [.product(name: product, package: identity)], path: ".",
                                sources: ["SwiftSupport.swift", "SwiftLive.swift", side == "old" ? "SwiftOld.swift" : "SwiftNew.swift"])]
)
