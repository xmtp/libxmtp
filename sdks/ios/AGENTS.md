# XMTP iOS SDK

Swift exports the generated `XmtpSdk` package from `crates/xmtp_sdk`.

## Commands

Run these commands from the repository root.

```bash
dev/nix-shell 'just ios build'          # Generate Swift and build the native XCFramework.
dev/nix-shell 'just ios check'          # Build the Swift package.
dev/nix-shell 'just example-ios build' # Build the example app for the simulator.
dev/nix-shell 'just ios check-consumer' # Bare-executable consumer and negative consumers.
dev/nix-shell 'just ios lint'
dev/nix-shell 'just ios format'
dev/nix-shell 'just ios test'           # Test the installed macOS package.
dev/nix-shell 'just ios test skip-lifecycle' # The same, without the lifecycle checks.
dev/nix-shell 'just ios test-lifecycle'     # Only ReaderTeardownTests and ListenerGateTests.
dev/nix-shell 'just ios test-host'        # Host-only loop: cargo debug macOS slice, no Nix build.
dev/nix-shell 'just ios test-simulator "platform=iOS Simulator,name=iPhone 17"'
dev/nix-shell 'just ios docs'
dev/nix-shell 'just ios test all --filter XmtpSdkTests.RecordCodecTests/testRemoteAttachmentLength'
NIX_DEVSHELL=ios dev/nix-shell 'ruby sdks/ios/script/test_podspec.rb'
```

The podspec test needs an existing Ruby runtime with `cocoapods-core`. Use the
same Ruby runtime as CocoaPods. It checks source selection, invalid receipts,
and simulator exclusions. It does not install or download a pod.

The recipe test checks the real Just commands without compiling the SDK.
The `test` recipe forwards extra Swift test arguments after its mode.

`check-consumer` stages a consumer package of the root package under
`target/sdk-ios-consumer`. It runs `Tests/Consumer/main.swift` as a bare
executable: without a bundle identifier, default storage fails with
`StorageLocationRequired`. Then it compiles each file in
`Tests/Consumer/Negative` and checks that the compile fails with the expected
diagnostics. The negative consumers check typed IDs, typed codec values, and
`Sendable` codec values. They are not part of the test target.

`RuntimeFakes.swift` in `Tests/XmtpSdkTests` replaces the generated Rust-backed
objects with fakes, so a test can run the Swift runtime without a backend. The
listener gate, reader iterator and event iterator tests use it. These tests and
`AppleLifecycleTests` need only the XCFramework from `just ios build`:

```bash
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter "XmtpSdkTests.(ListenerGateTests|ReaderIteratorReadTests|EventIteratorReadTests|AppleLifecycleTests)"'
```

## Rules

- Darwin only. The iOS recipes use `NIX_DEVSHELL=ios`.
- `dev/bindings` builds the XCFramework that CI caches when the selected Xcode
  (`xcode-select -p`) is `lib.xcode` from `nix/lib/ios-env.nix`. In CI another
  Xcode is an error. On a developer Mac, which names its install
  `/Applications/Xcode.app`, it first substitutes the CI output
  (`--max-jobs 0`) and builds the `-local` output only when no cache holds it.
  It prints which one it used.
- The `-local` outputs are keyed `local`, not by Xcode version, so after an
  Xcode upgrade they reuse the outputs that the old Xcode built. To rebuild
  them, delete them and run `dev/bindings` again:

  ```bash
  rm -f target/ios-sdk
  dev/nix-shell 'nix derivation show -r .#ios-xcframeworks-fast-local .#ios-xcframeworks-local |
    jq -r "(.derivations // .)[] | select(.env.XMTP_XCODE == \"local\") | .outputs.out.path | \"/nix/store/\" + ltrimstr(\"/nix/store/\")" |
    xargs nix store delete'
  ```

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
- CI runs the SDK test and simulator recipes through `just backend ci`.
  This starts disposable native PostgreSQL, S3, and backend services.
- `test-ios.yml` has one iOS CI job. It runs `dev/bindings` once and fails
  when the tracked Swift under `Sources/` differs from the generated Swift
  (run `dev/bindings` and commit `Sources/`). Then it runs `test` (every test,
  the lifecycle checks too), `check-consumer`, and
  `test-simulator`. `dev/bindings` skips the xcframework copy when
  `Artifacts/.xcframework-store-path` names the same store path.
  A selected iOS suite gates aggregate `Test` in `ci.yml`.
- The Xcode recipes clear inherited `LD` before Xcode selects its linker driver.
- The example app lives in `apps/example-ios` and has its own build and CI recipes.
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

`dev/nix-shell 'just ios test all --filter XmtpSdkTests.LegacyMigrationTests'` runs
`LegacyMigrationTests` on macOS.
It uses the pinned encrypted database and WAL in the internal converter crate.
The required iOS unit job includes this test in the normal XCTest target.
