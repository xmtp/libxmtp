# Build process

Build artifacts: .kt file, and cross-compiled binaries. Currently using `cross`, but can be built natively by downloading relevant toolchains.

# Uniffi

# Async and concurrency

Rust allows you to set any async futures executor (scheduler) that you like. Tokio is a multi threaded future executor that we have been using. You need to specify the executor at every entry point (hence why we have #[tokio:test] and #[uniffi::export(async_executor=‘tokio’)]

Async can be multi threaded across foreign language. The foreign language executor (read: scheduler) can be configured to intelligently poll the future running in rust. How it works: https://github.com/mozilla/uniffi-rs/blob/734050dbf1493ca92963f29bd3df49bb92bf7fb2/uniffi_core/src/ffi/rustfuture.rs#L11-L18

Uniffi leans on native Rust to avoid data races.

https://mozilla.github.io/uniffi-rs/udl/interfaces.html
Objects must be wrapped in Arc<> which is marshaled back and forth between raw pointers. Uniffi/rust handles object destruction but it is possible foreign language has to handle it too
The foreign language could be multi threaded too.
Exposed methods must use interior mutability for write operations, and be both send + sync (foreign language might be multi threaded). Use Tokio syncing primitives (which are async) rather than std rust primitives (which are blocking) - as the latter can cause deadlocks: https://rust-lang.github.io/async-book/03_async_await/01_chapter.html#awaiting-on-a-multithreaded-executor

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

```swift
        .package(url: "../xmtp-rust-swift/", branch: "your-local-branch")
```

- Make a local commit in xmtp-rust-swift (must be a commit to get picked up by Swift Package) and reference it in Xcode
- Note: commits to the same branch will not be picked up by Xcode, you must right-click the Swift package and do "Update Package"
