# XMTP Android SDK

Kotlin. Wraps `bindings/mobile` through uniffi.

## Commands

```bash
just android build                      # native .so + Kotlin bindings, via Nix
just android check                      # bindings + gradle build
just android lint                       # spotless + android lint
just android format
just android test                       # bindings + unit tests (library/src/test)
dev/nix-shell 'cd sdks/android && ./dev/bindings && ./gradlew -p . library:testDebug --tests org.xmtp.android.library.CryptoTest'   # one unit test class
just android test-integration           # instrumented tests (library/src/androidTest). Needs an emulator.
```

## Gotchas

- Needs `just backend up`.
- Always run `./dev/bindings` before Gradle. Bare `./gradlew` tests stale `.so` files.
- `library/src/test` = JVM unit tests. `library/src/androidTest` = instrumented, emulator only.
