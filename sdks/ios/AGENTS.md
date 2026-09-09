# XMTP iOS SDK

Swift. Wraps `bindings/mobile` through a uniffi xcframework.

## Commands

```bash
just ios build                          # xcframework + Swift bindings, via Nix
just ios check                          # bindings + swift build
just ios lint                           # swiftlint + swiftformat --lint
just ios format
just ios test                           # bindings + macOS Swift tests
just ios test-simulator                 # bindings + iOS simulator tests
NIX_DEVSHELL=ios dev/nix-shell 'swift test --filter XMTPTests.ClientTests/testCreatesAClient'   # one test, from repo root
```

## Gotchas

- Darwin only. The `ios` just module defaults to `NIX_DEVSHELL=ios`.
- Start the backend with `just backend up`.
- Tests read `XMTP_BACKEND_URL`; the fallback is `http://localhost:5050`.
- On this Mac, export `XMTP_BACKEND_URL=http://127.0.0.1:5050`.
- CI supplies the URL of a Fly backend built from the tested commit.
- Pass an installed simulator with `just ios test-simulator "platform=iOS Simulator,name=iPhone 17"`.
- `Package.swift` is at the repo root. Run `swift` from the root, after `just ios build`.
