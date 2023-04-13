# XMTP Rust Swift

This repo builds a crate for iOS targets and packages it in an XMTPRustSwift.xcframework file.

It pairs with [xmtp-rust-swift](https://github.com/xmtp/xmtp-rust-swift) which is a tiny public repo hosting a Swift Package that wraps the XMTPRustSwift.xcframework produced here.

## Structure

- `Cargo.toml` here can reference any local crate in `libxmtp`, such as corecrypto or keystore
- `include/module.modulemap` - can remain untouched as it just imports the `xmtp_rust_swift.h` header
- `include/xmtp_rust_swift.h` - contains C function declarations that are exported into Swift land

## Prerequisites

- Rust
- Run `make download-toolchains` to get all the iOS and MacOS toolchains

## Workflow

- Write code in `./src` to expose functionality to Swift
- Run `cargo test` to make sure your code works
- Then update `include/xmtp_rust_swift.h` with any new functions or declarations
- Run `make pkg` to produce the XMTPRustFramework.xcframework

## Optional Steps for xmtp-ios integration

- Get set up with Xcode
- Clone [xmtp-ios](https://github.com/xmtp/xmtp-ios)
- Open Xcode, then open the `xmtp-ios` folder at the top-level in Xcode
- Follow README.md there to get Xcode roughly set up
- Try running a build, it should complain about no `XMTPRustSwift.xcframework` in the `xmtp-rust-swift` Swift package checkout in DerivedData
- Run `make push-to-derived` in this repo (libxmtp) to perform a hacky step which puts the xcframework in the right places in DerivedData
- Try building and testing again in Xcode and it should work
