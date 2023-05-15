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

## End-to-end React Native Walkthrough

Repositories you'll need:
- https://github.com/xmtp/xmtp-react-native
- https://github.com/xmtp/xmtp-ios
- https://github.com/xmtp/xmtp-rust-swift
- Our current repo: https://github.com/xmtp/libxmtp

Goal: add a "hello from Rust" function that a React Native app can call in JS (xmtp-react-native), that invokes Swift code (xmtp-ios), that calls into Rust code (xmtp-rust-swift + libxmtp/bindings_swift)

1. Add the "hello from Rust" function to `libxmtp/bindings_swift/lib.rs`

Since we're exposing a stateless and static function, we can use `sha256` and `keccak256` as examples. Notice in `lib.rs` the ffi module declarations for these functions
```
    extern "Rust" {
        fn sha256(data: Vec<u8>) -> Vec<u8>;
        fn keccak256(data: Vec<u8>) -> Vec<u8>;
+       fn hello_from_rust(name: String) -> String;
        ...
    }
```

And the implementations below
```
fn sha256(data: Vec<u8>) -> Vec<u8> {
    let result = hashes::sha256(data.as_slice());
    result.to_vec()
}

fn keccak256(data: Vec<u8>) -> Vec<u8> {
    let result = hashes::keccak256(data.as_slice());
    result.to_vec()
}

+ fn hello_from_rust(name: String) -> String {
+     format!("Hello, {} from Rust!", name)
+ }
```

2. Build a new XMTPRustSwift.xcframework and push it to the `xmtp-rust-swift` repo
- Make sure you're in `bindings_swift` crate
- After you make your code changes (and maybe even add a unit test) the utter: `make swift` to build and push the XMTPRustSwift.xcframework file to xmtp-rust-swift

Possible issues: You need toolchains (run `make download-toolchains` from above)

3. Go into `xmtp-rust-swift` and check that there are diffs `git diff`
- Create a new local branch like `git checkout -b my_local_hello_from_rust`
- Add and commit the new changeset (similar but not identical to):
  ```
	modified:   Sources/XMTPRust/xmtp_rust_swift.swift
	modified:   XMTPRustSwift.xcframework/Info.plist
	modified:   XMTPRustSwift.xcframework/ios-arm64/Headers/Generated/xmtp_rust_swift/xmtp_rust_swift.h
	modified:   XMTPRustSwift.xcframework/ios-arm64/libxmtp_rust_swift.a
	modified:   XMTPRustSwift.xcframework/ios-arm64_x86_64-simulator/Headers/Generated/xmtp_rust_swift/xmtp_rust_swift.h
	modified:   XMTPRustSwift.xcframework/ios-arm64_x86_64-simulator/libxmtp_rust_swift.a
	modified:   XMTPRustSwift.xcframework/macos-arm64_x86_64/Headers/Generated/xmtp_rust_swift/xmtp_rust_swift.h
	modified:   XMTPRustSwift.xcframework/macos-arm64_x86_64/libxmtp_rust_swift.a
    modified:   Sources/XMTPRust/xmtp_rust_swift.swift
  ```

4. Now switch repos again to `xmtp-ios`
- Open xmtp-ios in Xcode as a directory. Do not open the `example/*.xcodeproject` file as the entrypoint
- You should see Swift packages syncing on the left side
- Open up `Package.swift` and replace the xmtp-rust-swift branch (see a few sections above for xmtp-ios integration)
- Run the unit tests in Xcode, at this point the steps are standard to xmtp-ios (not Rust-integration specific) and documented in the xmtp-ios README.md
- Find a location in the code that uses `XMTPRust`'s sha256 or keccak256 functions. Add an additional call to `hello_from_rust`.
- Test that the call compiles correctly and emits the correct string

5. OPTIONAL: You want to push a Cocoapod
- There are two cocoapods in play. Cocoapod `XMTPRust` comes from the `xmtp-rust-swift` repo and `XMTP` comes from `xmtp-ios` and depends on `XMTPRust`
- Make sure your local branch of xmtp-rust-swift passes `pod lib lint XMTPRust.podspec --allow-warnings`
- To push the `XMTPRust` cocoapod, go to `xmtp-rust-swift` and bump the version in the `XMTPRust.podspec` file to your intended one
- Open a PR, get it merged (push branches remotely with care, as the framework files form a 40MB diff, I only do it when I'm very confident in local changes)
- Then tag your commit on main `git checkout main && git pull && git tag X.X.X-yyyyyy`, which should match the version in the podspec
- Then push the tag up `git push origin --tags`
- Then release the cocoapod: `pod trunk push XMTPRust.podspec --allow-warnings`
- You should get an email when the release is successful
- If you don't have permission, register your email with Cocoapods and then have someone with permission add you.
- It may take up to hours after the release for your new version to be ready

[Search Query for XMTPRust Pod Versions](https://github.com/search?q=repo%3ACocoaPods%2FSpecs+XMTPRust&type=commits)
[Search Query for XMTP Pod Versions](https://github.com/search?q=repo%3ACocoaPods%2FSpecs+XMTP&type=commits)

NOTE: The XMTP cocoapod is pushed from xmtp-ios and requires documentation in that repository. Make sure to update the podspec dependency in `XMTP.podspec` that references `XMTPRust`.
