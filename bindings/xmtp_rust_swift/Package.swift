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
                url: "https://raw.githubusercontent.com/michaelx11/build_files/main/swift_bundle_hosting/bundle.zip",
                checksum: "b8751114bcdd405219f74a1a2f623b8a35e2007e29176fdf373fb119a91a2a60"),
        ]
)
