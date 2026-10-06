# XMTP Android SDK

Kotlin package `uniffi.xmtp_sdk`. The SDK uses generated bindings and runtime from
`xmtp_sdk`. The version is 8.0.0. Main source adds Android storage, process
lifecycle, and file log helpers.

## Commands

`just android check` and `just android test` stage the matched host SDK library for native JVM calls.
The test runtime includes host JNA. For `test-unit` with existing generated output,
set `JAVA_TOOL_OPTIONS=-Djna.library.path=PATH` to the matched host SDK library
directory.

Run from the repository root. Each recipe uses the Android Nix shell.
Build tools use normal parallelism and preserve caller job settings.
The format recipe uses strict dependency verification and stops its Gradle daemon.
The dependency locks include the pinned Spotless formatter graph.
The config check tests settings service startup and clock failure before emulator tests.

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
dev/nix-shell 'just android check-consumers'
dev/nix-shell 'just android docs'
```

The Android floor is API 23. Generated timestamps use `java.time.Instant`. Keep
core library desugaring on in the library and app consumers, with pinned
`com.android.tools:desugar_jdk_libs:2.1.5`. Dependency locks and SHA256 Gradle
verification metadata cover the final resolved graph.

`test-min-sdk` requires a Linux x86_64 runner. It loads release JNI and checks
generated `Instant` and `Date` conversions on API 23. It also creates public
clients with explicit and in-memory storage, then closes them. It also cancels a
public reader collector and checks unacknowledged replay. Start the normal
backend before this route. Kotlin generation uses the stock Android cleaner
mode: JNA below API 34 and `SystemCleaner` on API 34 or later. The Nix emulator launcher
checks the guest API and synchronizes its clock before it starts the test.
Both emulator test recipes run their commands inside the launcher's owned scope.
The launcher stops that emulator and its test process group, then removes its
temporary Android home on success, failure, or TERM/INT cancellation. It preserves
the command's exit status and retains diagnostics outside the removed home.
Integration `adb reverse` targets the scope's selected `ANDROID_SERIAL`.
The standalone `run-test-emulator` command retains the ready emulator for
interactive reuse; use `run-test-emulator -- COMMAND ARGS...` for automatic teardown.

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

`library/src/test` has JVM tests. Most use recording fakes of the generated
classes (`Group(NoHandle)`). The live JVM tests read `XMTP_BACKEND_URL`, and
the stream recovery and attachment lifetime tests read `XMTP_TOXIPROXY_API` and
`XMTP_BACKEND_TOXIC_URL`; `just android test` loads them from the worktree
environment. Those two tests change the shared `backend` proxy and reset it on
exit; do not run them at the same time as other Toxiproxy tests in the
worktree. Live tests create clients through `withClients`, which ends each
client on every exit. `library/src/androidTest` has installed Android tests.
Host JVM checks do not prove an Android AAR loads.

`library/src/test/negative` holds negative consumers. They are not compiled
with the tests. `check-consumers` adds one at a time to the unit test sources
(the `xmtpNegativeConsumer` Gradle property) and checks that the compile fails
with the expected diagnostics: typed IDs, typed content, Group and Dm types,
and typed codec values.

Instrumentation has no foreground Activity. Its fixtures disable
`AndroidStreamLifecycle.enabled`, resume native streams, and restore the flag.
`AndroidContextStartupTest` registers the process observer and then sends real
`ON_STOP` and `ON_START` events to `ProcessLifecycleOwner`. It leaves the process
started, so later tests keep live streams.
Android client Context overloads resolve default storage under `filesDir/xmtp_db`
and enable process lifecycle control by default. Explicit storage stays explicit.
End clients in `withContext(NonCancellable)`.

## Message delivery

A native message reader acknowledges the previous message at the next `next()`
call. With direct sequential Flow collection, the collector callback finishes
before acknowledgement starts. A buffer or another asynchronous operator
can let `emit` return before downstream processing ends. Cancellation after the
acknowledgement does not restore the message to default progress. Do not claim
durable acknowledgements for each downstream consumer. Cancellation closes the
reader in `NonCancellable`; replay is preserved before ACK commit admission.

Only one default message reader can own progress in a client database. Different
group or DM scopes do not create separate default owners. A second active default
message reader fails with `XmtpException.ConsumerOwned`. Explicit `from` cursors
permit independent replay/live readers that do not advance default progress. Use the
generated reader options for scopes, filters, and replay. Keep typed errors and
`ULong` values.
