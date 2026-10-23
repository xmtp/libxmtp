# XMTP Android SDK

Kotlin `uniffi.xmtp_sdk` wraps generated Rust bindings; handwritten code owns
Android storage, lifecycle, and file logging.

```sh
dev/nix-shell 'just android check'
dev/nix-shell 'just android test-host' # debug host loop
dev/nix-shell 'just android test'
dev/nix-shell 'just android check-consumers'
```

- API 23 minimum; keep core library desugaring. This root owns the toolchain,
  Gradle wrapper, and shared dependency verification; app locks stay in the app.
  Refresh with strict verification and check both SDK and app graphs.
- Bindings and JNI must match. Host tests do not prove installed AAR loading;
  preserve emulator and all-ABI checks. For packaging, dependency refresh,
  minimum API, and emulator work, read [build/test procedures](DEVELOPMENT.md).
- Before publishing, read [Android release wiring](../../.github/RELEASING.md#android-publication).
- Use `withClients`; end clients in `NonCancellable`. Live tests need the backend.
  Proxy tests must not overlap other tests using the worktree's shared proxy.
- Device tests cover Android wiring; Rust owns SDK feature tests. Negative
  consumers must fail at each invalid call.
- Flow buffering can acknowledge before downstream processing finishes; do not
  promise per-consumer durable acknowledgement. Default readers share one owner
  per database. Explicit cursors replay without advancing default progress.
