# XMTP Android SDK

Kotlin package `uniffi.xmtp_sdk`. The SDK uses generated bindings and runtime from
`xmtp_sdk`. The version is 8.0.0. Main source adds Android storage, process
lifecycle, and file log helpers.

## Commands

`just android test` stages the matched host SDK library for native JVM calls.
The test runtime includes host JNA. For `test-unit` with existing generated output,
set `JAVA_TOOL_OPTIONS=-Djna.library.path=PATH` to the matched host SDK library
directory.

Run from the repository root. Each recipe uses the Android Nix shell.
Build tools use normal parallelism and preserve caller job settings.
The format recipe uses strict dependency verification and stops its Gradle daemon.
The dependency locks include the pinned Spotless formatter graph.

```bash
dev/nix-shell 'just android build'
dev/nix-shell 'just android assemble'
dev/nix-shell 'just android check'
dev/nix-shell 'just android lint'
dev/nix-shell 'just android format'
dev/nix-shell 'just android test'
dev/nix-shell 'just android test-unit --tests uniffi.xmtp_sdk.AndroidStreamLifecycleTest'
dev/nix-shell 'just android test-integration'
dev/nix-shell 'just android test-min-sdk'
dev/nix-shell 'just android docs'
```

The Android floor is API 23. Generated timestamps use `java.time.Instant`. Keep
core library desugaring on in the library and app consumers, with pinned
`com.android.tools:desugar_jdk_libs:2.1.5`. Dependency locks and SHA256 Gradle
verification metadata cover the final resolved graph.

`test-min-sdk` requires a Linux x86_64 runner. It loads release JNI and checks
generated `Instant` and `Date` conversions on API 23. The Nix emulator launcher
checks the guest API and synchronizes its clock before it starts the test.

`dev/bindings` stages `android-sdk-libs-fast`. `dev/bindings --release` stages
`android-sdk-libs` with arm64-v8a, armeabi-v7a, x86_64, and x86 JNI libraries.
Gradle uses the matched generated sources and contract. The release AAR contains
`libxmtp_sdk.so` and `assets/sdk-contract.json`.

For an already generated package, set `XMTP_SDK_GENERATED_DIR` to its root and
`XMTP_SDK_ANDROID_JNI_DIR` to its `jniLibs` directory. This skips native generation
when Gradle runs directly through `dev/nix-shell`. The `test-unit` recipe uses
these existing matched bindings and accepts Gradle test filters.

## Local services

Run `dev/nix-shell 'just backend up'`. The library test BuildConfig reads backend
and anvil ports from the worktree environment. The emulator reaches these
services through `10.0.2.2`. Attachments use the backend's advertised loopback URL.
Set `XMTP_ANDROID_BACKEND_URL` to use a test relay or another reachable endpoint.
The integration recipe uses `adb reverse` for `XMTP_S3_PORT`; it must match the
backend attachment URL. Tests set `allowPrivateNetwork = true` for this fixture.

## Tests and lifecycle

`library/src/test` has JVM helper tests. `library/src/androidTest` has installed
Android tests. Kotlin conformance under `crates/xmtp_sdk/conformance/kotlin`
checks the generated runtime. Host JVM checks do not prove an Android AAR loads.

Instrumentation has no foreground Activity. Its fixtures disable
`AndroidStreamLifecycle.enabled`, resume native streams, and restore the flag.
Android client Context overloads resolve default storage under `filesDir/xmtp_db`
and enable process lifecycle control by default. Explicit storage stays explicit.
End clients in `withContext(NonCancellable)`.

## Message delivery

A native reader acknowledges the previous message at the next `next()` call.
The direct Flow collector return is the collection boundary. App buffering has
its own boundary. Cancellation closes the reader in `NonCancellable` and keeps
unacknowledged messages available for replay. Use the generated reader options
for scopes, filters, and replay. Keep typed errors and `ULong` values.
