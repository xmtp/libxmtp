# XMTP iOS SDK

Darwin only. `Package.swift` is at the repository root.

```sh
dev/nix-shell 'just ios test-host' # debug host loop
dev/nix-shell 'just ios test'      # release package proof
dev/nix-shell 'just ios check-consumer'
dev/nix-shell 'just ios lint'
```

- `Sources/XmtpSdk/xmtp_sdk.swift` and `runtime` are generated. Change Rust or
  bindgen, regenerate, and commit tracked sources. Never commit the XCFramework.
- Read [build/test procedures](DEVELOPMENT.md) for Xcode/cache issues, simulator,
  lifecycle, podspec, and minimum-deployment checks. Host debug passes do not
  replace release/simulator checks.
- Tests need the worktree backend. Create clients with `withClients` or
  `withLiveClients`; end them on every exit. Use `pause(seconds:)`, not newer
  APIs such as `Task.sleep(for:)`: `swift test` raises the deployment target.
- Streams own reader/callback lifetimes; Rust owns durable cursors and replay.
  Close/cancel/release must not acknowledge a pending message.
- Release SwiftPM and CocoaPods use the same URL/SHA256 in `ReleaseArtifacts.json`;
  never invent checksums.
