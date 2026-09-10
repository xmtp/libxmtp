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
just ios docs                           # bindings + static DocC reference
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

## Message delivery

- Message streams use a one-item mailbox. Queue insertion does not acknowledge a message. A new iterator request acknowledges the previous item.
- `MessageReader.next()` uses the same boundary. Close the reader when finished. Close, cancellation, and release of the full stream do not acknowledge pending items.
- `AsyncThrowingStream` shares sequence and iterator storage. Releasing only the iterator does not close a sequence that the app still retains.
- `messageReader(from: cursor)` opens independent replay. `messageHistorySnapshot` returns messages and a cursor from one database snapshot. Each delivered message has a typed `deliveryCursor`.
- Readers expose scope and filter updates, catch-up snapshots, and change waits. Catch-up keeps the current generation and at most one previous generation.
- `ClientOptions.streamSettings` accepts optional limits and millisecond timers. Omitted fields use core defaults. Native client creation validates all values.
- Read `error.streamFailureDetails` for typed barrier, publish-confirmation, and catch-up failures. A nil target means capture failed; zero is a captured empty target. All cursors and counts remain `UInt64` values.
