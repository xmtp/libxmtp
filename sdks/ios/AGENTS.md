# XMTP iOS SDK

Swift. Wraps `bindings/mobile` through a uniffi xcframework.

## Commands

```bash
just ios build                          # xcframework + Swift bindings, via Nix
just ios check                          # bindings + swift build
just ios lint                           # swiftlint + swiftformat --lint
just ios format
just ios test                           # bindings + swift test
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter XMTPTests.ClientTests/testCreatesAClient'   # one test, from repo root
```

## Gotchas

- Darwin only. The `ios` just module defaults to `NIX_DEVSHELL=ios`.
- Needs `just backend up`.
- `Package.swift` is at the repo root. Run `swift` from the root, after `just ios build`.
