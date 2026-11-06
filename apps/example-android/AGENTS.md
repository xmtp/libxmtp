# XMTP Messenger Android

```sh
dev/nix-shell 'just example-android check-all' # builds, lint, unit tests
dev/nix-shell 'just example-android test-integration' # backend + scoped emulator
dev/nix-shell 'just example-android preview-check' # shared UI only
```

- Use the in-tree SDK composite build, never a published package. `:example`
  maps to `app`, `:example-shared` to `shared`. Common Compose code has no SDK objects.
- Preserve application ID and private profile paths. SDK owns shared toolchain,
  wrapper, and verification inventory; app dependency locks stay here.
- After APK dependency/plugin edits, run `just example-android apk-update-deps`
  through Nix. Never commit Firebase config or private signing keys.
- Instrumentation targets Debug and an owned emulator. Keep the fixture lease
  admission before Gradle and in metadata/group-settings tests. Use the
  [fixture guide](fixtures/README.md) for setup/cleanup and
  [performance guide](performance/README.md) for target admission.
- Host tests/previews do not prove installed package loading. Preserve SDK
  consumer/device checks. Commands, APK distribution, previews, and service
  forwarding: [README](README.md#build-and-check).
