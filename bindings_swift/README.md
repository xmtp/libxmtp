# XMTP Rust Swift

This repo builds a crate for iOS targets and packages it in an XMTPRustSwift.xcframework file.

It pairs with [xmtp-rust-swift](https://github.com/xmtp/xmtp-rust-swift) which is a tiny public repo hosting a Swift Package/Cocoapod that wraps the XMTPRustSwift.xcframework produced here.

## Structure

- `Cargo.toml` here can reference any local crate in `libxmtp`, such as `xmtp_crypto` or `xmtp_networking`
- `include/module.modulemap` - can remain untouched as it just imports the `xmtp_rust_swift.h` header

## Prerequisites

- Rust
- Run `make download-toolchains` to get all the iOS and MacOS toolchains
- Clone `xmtp-rust-swift` from above and put it at the same directory level as this repository (so `../xmtp-rust-swift`)

## Workflow

- Write code in `./src` to expose functionality to Swift
- Run `cargo test` to make sure your code works
- Run `make swift` to build local crate, generate Swift bindings, package the xcframework, and push all files to `../xmtp-rust-swift`

### Just xcframework
- Run `make framework`

## Optional Steps for xmtp-ios integration

- Get set up with Xcode
- Clone [xmtp-ios](https://github.com/xmtp/xmtp-ios)
- Open Xcode, then open the `xmtp-ios` folder at the top-level in Xcode
- Follow README.md there to get Xcode roughly set up
- Go to `Package.swift` and find the dependencies section. Add:
```
        .package(url: "../xmtp-rust-swift/", branch: "your-local-branch")
```
- Make a local commit in xmtp-rust-swift (must be a commit to get picked up by Swift Package) and reference it in Xcode
- Note: commits to the same branch will not be picked up by Xcode, you must right-click the Swift package and do "Update Package"
