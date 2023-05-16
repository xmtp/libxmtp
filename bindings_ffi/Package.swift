// swift-tools-version:5.3
import PackageDescription
import Foundation
let package = Package(
        name: "XMTPRustSwift",
        platforms: [
            .iOS(.v13), 
            .macOS(.v11)
        ],
        products: [
            .library(
                name: "XMTPRustSwift",
                targets: ["XMTPRustSwift"]),
        ],
        targets: [
            .binaryTarget(
                name: "XMTPRustSwift",
                path: "XMTPRustSwift.xcframework"
            ),
        ]
)
