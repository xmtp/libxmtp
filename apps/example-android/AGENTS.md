# XMTP Messenger Android example

Run commands from the repository root. These recipes use the Android Nix shell.

```bash
dev/nix-shell 'just example-android start'
dev/nix-shell 'just example-android apk'
dev/nix-shell 'just example-android apk-update-deps'
dev/nix-shell 'just example-android check'
dev/nix-shell 'just example-android check-all' # unit tests, lint, and builds in one Gradle call (CI)
dev/nix-shell 'just example-android preview-check'
dev/nix-shell 'just example-android lint'
dev/nix-shell 'just example-android format'
dev/nix-shell 'just example-android test'
dev/nix-shell 'just example-android test-integration'
dev/nix-shell 'just example-android metadata-fixture-test'
dev/nix-shell 'just example-android metadata-fixture-test MetadataBackendFixtureTest.test_every_psql_call_uses_a_private_passfile_without_argv_password'
dev/nix-shell 'just example-android metadata-fixture-smoke'
dev/nix-shell 'python3.11 -B -m unittest discover -s apps/example-android/performance -p "test_*.py"'
```

Open this directory as a Gradle project in Android Studio. `:example` maps to
`app`; `:example-shared` maps to `shared`. The composite build maps
`org.xmtp:android` to `sdks/android/library` in this checkout. Do not use a
published SDK package for these checks.

`apk` builds the `example-android-apk` Nix derivation, then copies its signed
universal APK to `build/apk/xmtp-messenger-local.apk`. It starts no services or
emulator. The derivation includes all four JNI ABIs and checks packaging and
signing. It uses the non-debuggable release variant and its HTTPS transport rules.
The APK uses the public AOSP testing key under the separate
`org.xmtp.android.example.testing` app ID, labelled `XMTP Messenger (Testing)`.
The Nix caller sets `xmtpExampleTestingApk=true` for this identity. The APK embeds the SDK version
from `sdks/android/gradle.properties`. For releases, `XMTP_EXAMPLE_VERSION` selects
the output filename and must match that version. Android releases attach it to
the `android-<version>` GitHub Release.

`apk-update-deps` refreshes `nix/package/example-android-apk-deps.json` using a
fresh Gradle cache and strict dependency verification. Run it after changing the
APK's dependencies or build plugins. The Nix build replays only these pinned
downloads. It does not read the developer's Gradle cache, Firebase file, signing
key, or worktree service configuration. Firebase is off in the downloadable APK.

The SDK root owns `gradle/toolchain.properties`, the Gradle wrapper, and the
shared dependency verification inventory. App module locks remain here. The
app uses API 27 and JVM 17. Common Compose source has no SDK objects.
Keep the application ID and private profile paths unchanged.

`preview-check` compiles the shared UI and Android Studio previews. It does not
build native bindings or start services. Use `MessengerPreviews.kt` for fixed
screen and attachment states at phone, dark, 320 dp, 200% text, RTL, landscape,
and wide window sizes. The app's `NativeReactionPickerPreviews.kt` renders the
actual Android emoji picker and search. It needs the regular app build;
`preview-check` covers only the shared module. These previews start no session
or services. A preview is not a runtime proof.

`check` compiles Debug, Release, shared targets, and the app test APK with strict
verification. `test` stages matched bindings and the host JNI library before
host tests. A host test pass does not prove Android package loading.
`check` also checks JNI stripping and dynamic symbols in both app APKs.
The Off Firebase graph uses `app/gradle.lockfile`. The configured graph uses
`app/firebase-gradle.lockfile`. Supply `XMTP_FIREBASE_CONFIG` or place the
private file at `app/google-services.json`. Do not commit it.

## Local services

`start` builds the local SDK, boots a visible API 34 emulator at 1080 × 2400
and 420 dpi with automatic GPU rendering, installs Debug,
forwards the worktree backend and object-store ports, and launches the app.
It uses the existing backend. Press Enter to stop the owned emulator and remove
its temporary device data. Keep the scoped launcher so failure and interruption
also clean up only this device.

Instrumented tests start from Chats: `TestAppGraphRule` turns off Choose your
handle unless a test passes `offerHandleChoice = true`.

The Android host is in `app`. Shared Compose screens are in `shared/src/commonMain`.
`test` runs host and shared unit tests. They create no client and need no
backend. `test-integration` runs the app instrumentation smoke in the owned
emulator scope. Instrumented tests run only against the debug build. No device
test runs the release build; README "Connect" states its transport rules. Run
both host variants with
`dev/nix-shell 'just example-android test :example:testReleaseUnitTest'`.

The instrumentation smoke has 13 tests: group settings (presets, members and
roles, pending removal), recipient handles (6), a 64 MiB attachment verified on
another client, an offline reopen that keeps local history, app metadata fields
saved and refreshed, and the default reader released when the activity stops.
SDK behavior is tested in Rust; these tests check the app wiring on a device.

The recipe starts a catalogue backend with its own database and listeners
(`fixtures/metadata_backend.py`). The Android shell supplies the pinned
PostgreSQL client and health probe. The fixture passes `metadataBackendUrl` to
instrumentation. The recipe forwards that port, the worktree backend port, and
`XMTP_S3_PORT` for signed loopback attachment URLs.
The route checks the private lease with `performance/target.py` before Gradle
starts and passes `metadataBackendOwned=true`. Group settings and metadata
tests accept only that owned loopback target. Keep both admission checks after
a source move.
It keeps shared backend URLs for the other tests. It removes only its database
and process groups on success, failure or cancellation. Set
`XMTP_METADATA_LOG_DIR` to retain backend logs at a selected path. The local
`metadata-fixture-test` recipe checks cleanup with process stubs; `just test dev`
also runs it. `metadata-fixture-smoke` checks real backend startup and cleanup
without an emulator. Start the backend before app instrumentation. Keep SDK package
and consumer tests.

Run `dev/nix-shell 'just backend up'`. The library test BuildConfig reads backend
and anvil ports from the worktree environment. The emulator reaches these
services through `10.0.2.2`. Attachments use the backend's advertised loopback URL.
Set `XMTP_ANDROID_BACKEND_URL` to use a test relay or another reachable endpoint.
The integration recipe uses `adb reverse` for `XMTP_S3_PORT`; it must match the
backend attachment URL. Tests set `allowPrivateNetwork = true` for this fixture.

`performance/target.py` admits only the fixture's active private lease; see
`performance/README.md`. The `unittest discover` command above runs its tests.
It starts no backend, device, or SDK build.

## Verification

CI uses `test-example-android.yml`, which also runs app lint. SDK package,
public consumer, API 23, and all-ABI proofs stay in their SDK workflows.

`just test dev` runs the fixture and lease-admission tests, CI-routing, and
command-path regressions. A source move does not establish a new runtime proof.
