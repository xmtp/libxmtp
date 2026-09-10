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
- Emulator tests use `localApi()`, which reads `BuildConfig.XMTP_BACKEND_URL`. `library/build.gradle` sets it from `XMTP_BACKEND_PORT`, so each worktree reaches its own backend. The main checkout resolves to `http://10.0.2.2:5050`.
- Smart contract wallet tests use anvil at `http://10.0.2.2:8545`.
- Supply `ClientOptions.Api(backendUrl = "http://10.0.2.2:5050")`. The URL has no default. The optional `env` string selects the database file alias.

## Test requirements

- Run `./dev/bindings` before Gradle compilation or tests. It builds the native libraries and matching Kotlin bindings.
- `library/src/test` contains JVM unit tests.
- `library/src/androidTest` contains instrumented tests. These tests need a running backend and an emulator.
- Instrumented fixtures disable automatic stream lifecycle handling and resume streams. They restore the setting after each test. There is no foreground Activity to keep streams active.
