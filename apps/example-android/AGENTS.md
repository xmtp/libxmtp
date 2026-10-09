# XMTP Messenger Android example

Run commands from the repository root. These recipes use the Android Nix shell.

```bash
dev/nix-shell 'just example-android check'
dev/nix-shell 'just example-android lint'
dev/nix-shell 'just example-android format'
dev/nix-shell 'just example-android test'
dev/nix-shell 'just example-android test-integration'
dev/nix-shell 'just example-android test-release-integration'
dev/nix-shell 'just example-android support-fixture'
dev/nix-shell 'just example-android io-fixture'
dev/nix-shell 'just example-android metadata-fixture-test'
dev/nix-shell 'just example-android metadata-fixture-test MetadataBackendFixtureTest.test_every_psql_call_uses_a_private_passfile_without_argv_password'
dev/nix-shell 'just example-android metadata-fixture-smoke'
dev/nix-shell 'just example-android metadata-fixture-create-test'
dev/nix-shell 'just example-android performance-check'
dev/nix-shell 'just example-android performance'
```

Open this directory as a Gradle project in Android Studio. `:example` maps to
`app`; `:example-shared` maps to `shared`. The composite build maps
`org.xmtp:android` to `sdks/android/library` in this checkout. Do not use a
published SDK package for these checks.

The SDK root owns `gradle/toolchain.properties`, the Gradle wrapper, and the
shared dependency verification inventory. App module locks remain here. The
app uses API 27 and JVM 17. Common Compose source has no SDK objects.
Keep the application ID and private profile paths unchanged.

`check` compiles Debug, Release, shared targets, and the app test APK with strict
verification. `test` stages matched bindings and the host JNI library before
host tests. A host test pass does not prove Android package loading.
The Off Firebase graph uses `app/gradle.lockfile`. The configured graph uses
`app/firebase-gradle.lockfile`. Supply `XMTP_FIREBASE_CONFIG` or place the
private file at `app/google-services.json`. Do not commit it.

## Local services

The Android host is in `app`. Shared Compose screens are in `shared/src/commonMain`.
`test` runs host and shared unit tests. `test-integration` runs app instrumentation
in the owned emulator scope.
It forwards the current worktree backend and S3 ports for signed loopback URLs.
The app test scope owns a loopback TCP relay for S3 GET response admission.
`io-fixture` checks its listener startup and teardown without a device.
It preserves real S3 content and signed headers, with no connection reuse.
The cancellation test controls only
this relay. Its wrapper removes the listener and connections after the child
scope exits. It does not change backend or shared fault-proxy configuration.
`test-release-integration` tests the actual release build with temporary
local test signing. It keeps DEBUG=false and the release resources. It forwards
the backend port for the release loopback connection. The Gradle property
`xmtpExampleReleaseTests=true` selects this test mode. It does not change URL
admission. Run both host variants with
`dev/nix-shell 'just example-android test :example:testReleaseUnitTest'`.
It also forwards the backend proxy and its API. It supplies the `toxicBackendUrl`
and `toxiproxyApi` runner arguments from the worktree environment. Attachment
interruption tests change only their named toxic and restore the backend proxy.
The app integration route also starts an owned disposable PostgreSQL/backend
fixture with no attachment target. Docker assigns its published port. The route
forwards it and supplies `unsupportedBackendUrl`. The fixture removes only its
owned containers and network and retains its logs after success or failure.
Run this route alone when using a shared stack; no other proxy test can run at
the same time. Caller environment values can select an existing stack.
The recipe also starts a catalogue backend with its own database and listeners.
The Android shell supplies the pinned PostgreSQL client and health probe. The
fixture passes `metadataBackendUrl` to instrumentation and forwards that port.
It keeps shared backend URLs for the other tests. It removes only its database
and process groups on success, failure or cancellation. Set
`XMTP_METADATA_LOG_DIR` to retain backend logs at a selected path. The local
`metadata-fixture-test` recipe checks cleanup with process stubs; `lint-config`
also runs it. `metadata-fixture-smoke` checks real backend startup and cleanup
without an emulator. Start the backend before app instrumentation. Keep SDK package
and consumer tests.

Run `dev/nix-shell 'just backend up'`. The library test BuildConfig reads backend
and anvil ports from the worktree environment. The emulator reaches these
services through `10.0.2.2`. Attachments use the backend's advertised loopback URL.
Set `XMTP_ANDROID_BACKEND_URL` to use a test relay or another reachable endpoint.
The integration recipe uses `adb reverse` for `XMTP_S3_PORT`; it must match the
backend attachment URL. Tests set `allowPrivateNetwork = true` for this fixture.

The performance recipe requires Linux x86_64 with KVM. It owns an API 34
x86_64 emulator with four CPUs and 4096 MiB RAM. It seeds 1000 conversations
and 100000 Published messages through public SDK sends. It runs five warmups
and 30 measured samples, checks heap and cache bounds, then removes cache
eviction and requires its named assertion to fail. It restores the same source
and measures the same dataset again. Seed progress and `seedMs` stay in the
proof logs. The fixed job timeout is provisional until measured seed progress
sets the final limit. Host validator tests also run under `lint-config`.
See `performance/README.md` for budgets and retained proof files.

## Verification

CI uses `test-example-android.yml` and `lint-example-android.yml`. SDK input
changes also select these compatibility gates. SDK package, public consumer,
API 23, and all-ABI proofs stay in their SDK workflows.

`just lint-config` runs fixture, performance-tool, CI-routing, and command-path
regressions. Preserve their failure controls and the runtime proof ledger in
`verification.md`. A source move does not establish a new runtime proof.
