# XMTP Android SDK

Kotlin. Wraps `bindings/mobile` through uniffi.

## Commands

Run from the repository root. Each recipe uses the Android Nix shell.

```bash
just android build             # Build native libraries and Kotlin bindings.
just android assemble          # Compile the library, example, and instrumented tests.
just android check             # Build bindings and run the Gradle build.
just android lint              # Run Spotless and Android Lint.
just android format            # Format Kotlin code.
just android test              # Build bindings and run JVM unit tests.
just android test-integration  # Build bindings and run tests on an emulator.
just android docs              # Generate the Kotlin API reference.
dev/nix-shell 'cd sdks/android && ./dev/bindings && ./gradlew -p . library:testDebugUnitTest --tests org.xmtp.android.library.ClientCacheKeyTest'
```

## Local services

- Run `just backend up` for the main test stack.
- To use the published backend image, run `./dev/docker/up`.
- The shared `dev/docker/compose.yml` runs `db`, `replica`, `backend`, `anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`.
- `sdks/android/dev/local/compose` forwards commands to the shared stack.
- Emulator tests use `localApi()` with `http://10.0.2.2:5050`.
- Smart contract wallet tests use anvil at `http://10.0.2.2:8545`.
- Supply `ClientOptions.Api(backendUrl = "http://10.0.2.2:5050")`. The URL has no default. The optional `env` string selects the database file alias.

## Test requirements

- Run `./dev/bindings` before Gradle compilation or tests. It builds the native libraries and matching Kotlin bindings.
- `library/src/test` contains JVM unit tests.
- `library/src/androidTest` contains instrumented tests. These tests need a running backend and an emulator.
- Instrumented fixtures disable automatic stream lifecycle handling and resume streams. They restore the setting after each test. There is no foreground Activity to keep streams active.

## Message delivery

- Message streams keep native acknowledgement tokens through the SDK queue. The direct Flow collector return is the acknowledgement boundary. App-added buffering has a separate boundary.
- `MessageReader.next()` acknowledges the previous item, not the returned item. Close the reader when finished. Close and cancellation do not acknowledge pending items.
- `messageReader(from = cursor)` opens independent replay. `messageHistorySnapshot` returns messages and a cursor from one database snapshot. Each delivered message has a typed `deliveryCursor`.
- Readers expose scope and filter updates, catch-up snapshots, and change waits. Catch-up keeps the current generation and at most one previous generation.
- `ClientOptions.streamSettings` accepts optional limits and millisecond timers. Omitted fields use core defaults. Native client creation validates all values.
- Read `error.streamFailureDetails` for typed barrier, publish-confirmation, and catch-up failures. A null target means capture failed; zero is a captured empty target. All cursors and counts remain `ULong` values.
