# XMTP iOS SDK

Swift exports the generated `XmtpSdk` package from `crates/xmtp_sdk`.

## Commands

Run these commands from the repository root.

```bash
dev/nix-shell 'just ios build'          # Generate Swift and build the native XCFramework.
dev/nix-shell 'just ios check'          # Build the Swift package.
dev/nix-shell 'just ios check-examples' # Build both example apps for the simulator.
dev/nix-shell 'just ios lint'
dev/nix-shell 'just ios format'
dev/nix-shell 'just ios test'           # Test the installed macOS package.
dev/nix-shell 'just ios test-simulator "platform=iOS Simulator,name=iPhone 17"'
dev/nix-shell 'just ios docs'
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter XmtpSdkTests.RecordCodecTests/testRemoteAttachmentLength'
NIX_DEVSHELL=ios dev/nix-shell 'ruby sdks/ios/script/test_podspec.rb'
dev/nix-shell 'python3 sdks/ios/script/test_recipes.py'
```

The podspec test needs an existing Ruby runtime with `cocoapods-core`. Use the
same Ruby runtime as CocoaPods. It checks source selection, invalid receipts,
and simulator exclusions. It does not install or download a pod.

The recipe test checks the real Just commands without compiling the SDK.

`RuntimeFakes.swift` in `Tests/XmtpSdkTests` replaces the generated Rust-backed
objects with fakes, so a test can run the Swift runtime without a backend. The
listener gate, reader iterator and event iterator tests use it. These tests and
`AppleLifecycleTests` need only the XCFramework from `just ios build`:

```bash
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter "XmtpSdkTests.(ListenerGateTests|ReaderIteratorReadTests|EventIteratorReadTests|AppleLifecycleTests)"'
```

## Rules

- Darwin only. The iOS recipes use `NIX_DEVSHELL=ios`.
- Start the backend with `dev/nix-shell 'just backend up'`.
- Tests read `XMTP_BACKEND_URL`. The test recipe loads this worktree's URL.
- The stream recovery test sends its client through a loopback relay
  (`TestRelay.swift`). The live lifecycle tests suspend the process streams and
  resume them before they assert. The test that posts the UIKit background and
  foreground notifications runs only in `test-simulator`. Other clients on the
  backend are not affected.
- Tests compile for the package minimums (iOS 14, macOS 11). `swift test` raises
  the test deployment target, so it does not find an API that is too new. Use
  `pause(seconds:)` from `LiveBackend.swift`, not `Task.sleep(for:)`. To check,
  build the tests at the lowest target Xcode accepts:
  `NIX_DEVSHELL=ios dev/nix-shell "env -u LD xcodebuild build-for-testing -scheme XmtpSdk -destination 'generic/platform=iOS Simulator' ARCHS=arm64 IPHONEOS_DEPLOYMENT_TARGET=15.0"`.
- A live test creates its clients through `withClients` or `withLiveClients`
  (`LiveBackend.swift`). The helper ends every client on every exit, also after
  a throw or an early `return XCTFail(...)`. A test can still end a client
  itself; a second `end()` returns without an error.
- CI runs the test, example, and simulator recipes through `just backend ci`.
  This starts disposable native PostgreSQL, S3, and backend services.
- The Xcode recipes clear inherited `LD` before Xcode selects its linker driver.
- Example builds select `arm64` to match the shipped simulator library.
- Swift builds and tests use their default worker counts.
- `Package.swift` stays at the repository root.
- `Sources/XmtpSdk/xmtp_sdk.swift` and `Sources/XmtpSdk/runtime` are generated.
  Change Rust or `apps/xmtp_sdk_bindgen`, then generate them. Do not edit output.
- `Artifacts/XmtpSdkFFI.xcframework` is local build output. Do not commit it.
- The release job records one archive URL and SHA256 in `ReleaseArtifacts.json`.
  SwiftPM and CocoaPods read the same receipt. Do not invent a release checksum.
- Call `SDKClient.end()` when an app releases a client.

## Message delivery

A new iterator request acknowledges the previous message. Close, cancellation,
and release do not acknowledge a pending message. Rust owns replay and durable
cursors. Swift streams own their reader and callback lifetime. Use the generated
public records and typed errors. Do not add a second host cursor or content rule.
