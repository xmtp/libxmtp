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
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter XmtpSdkTests.RetainedBehaviorTests/testRemoteAttachmentLength'
NIX_DEVSHELL=ios dev/nix-shell 'ruby sdks/ios/script/test_podspec.rb'
dev/nix-shell 'python3 sdks/ios/script/test_recipes.py'
```

The podspec test needs an existing Ruby runtime with `cocoapods-core`. Use the
same Ruby runtime as CocoaPods. It checks source selection, invalid receipts,
and simulator exclusions. It does not install or download a pod.

The recipe test checks the real Just commands without compiling the SDK.

## Rules

- Darwin only. The iOS recipes use `NIX_DEVSHELL=ios`.
- Start the backend with `dev/nix-shell 'just backend up'`.
- Tests read `XMTP_BACKEND_URL`. The test recipe loads this worktree's URL.
- CI runs the test, example, and simulator recipes through `just backend ci`.
  This starts disposable native PostgreSQL, S3, and backend services.
- The Xcode recipes clear inherited `LD` before Xcode selects its linker driver.
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
