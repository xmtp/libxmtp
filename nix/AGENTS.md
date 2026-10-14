# Nix

Check an affected output with `nix build --no-link .#<output>`; run `dev/nix-shell 'just lint-config'`.

Rust-capable development shells select pinned Kache through `dev/kache-env`.
Check the package with `nix build --no-link .#kache` and the effective wrapper
with `dev/nix-shell --shell rust '"$RUSTC_WRAPPER" --version'`.
CI selects its wrapper through `setup-nix`. Nix build derivations retain their
Nix/Crane caches; they do not receive the outer shell's compiler cache.
Keep compiler credentials and writable host cache paths out of derivations.

On macOS, keep the compiler, linker, and SDK from the same toolchain. The local
and iOS shells select Xcode through `ios-env.nix` and use its native linker.
Do not use Nix's linker with the system SDK. The shells accept any Xcode. The
iOS build outputs are keyed to one Xcode (`xcode` in `ios-env.nix`, also
`lib.xcode`): a CI or release output fails to build with another, and the
`-local` outputs (`ios-xcframeworks-local`, `ios-xcframeworks-fast-local`)
build with any Xcode. A new Xcode changes `xcode` and every
`maxim-lobanov/setup-xcode` pin in `.github/workflows` together.
The focused Rust shell and Android host builds use the Nix toolchain.
They must replace native compiler and linker overrides from an outer local or
iOS shell. Otherwise, Xcode's linker uses Nix's SDK without its library search paths.
Run `dev/nix-shell 'bash dev/check-apple-toolchain'` in the local shell and
`NIX_DEVSHELL=rust dev/nix-shell 'bash dev/check-apple-toolchain'` for the Nix SDK.
Also check a shell transition with
`dev/nix-shell 'dev/nix-shell --shell rust "bash dev/check-apple-toolchain"'`.
Also check the Android shell transition with
`dev/nix-shell 'dev/nix-shell --shell android "bash dev/check-apple-toolchain"'`.
The check links `iconv`; a basic libc link does not detect mixed toolchains.

## Build isolation

- Cargo resolves every workspace member before applying package selection. Crane's `mkDummySrc` input needs each member's target sources, not just its manifest. Keep dependency-cache inputs narrow after dummy generation.
- Local protobuf generation needs schemas and the real proto build script in both dependency-only and final sources, plus build-platform `protoc` when cross-compiling. After changing source filters, verify that a schema edit changes the dependency derivation and an unrelated Rust source edit does not.
- Crane packages use `xmtp.hostToolchain`; only the iOS slices add their
  Apple targets. Each dependency layer runs its consumer's own Cargo command,
  with the same `-p` list, targets, profile, and environment;
  `base.mkCargoArtifacts` requires the command. Cargo unifies features across
  a `-p` list, so a final build with fewer packages recompiles the crates whose
  features differ. Before two consumers share a layer, compare
  `cargo tree -e normal,build --prefix none --format '{p}|{f}'` for each `-p`
  list alone and together.
- Backend builds restore only the shared dependency sources over the dummy workspace. Include migrations, SQL query files, and `apps/backend/.sqlx` metadata. Add each new shared dependency to the restored crate list. Build with `SQLX_OFFLINE=true`; no database is allowed during the build.
- The Rust shell provides sqlx-cli. Keep its version aligned with the backend SQLx dependency.
- SDK compiler sources use a dummy workspace and restore the selected local
  dependency graph. Keep embedded SDK data and bindgen templates in the real
  compiler source. Provenance is an evaluation-time digest of the SDK and
  bindgen closure sources (`lib/sdk-sources.nix`, `.#sdkProvenance`). Receipts
  record it and name files by name, so a product references no source and
  keeps its path on a commit outside the closure. The cargo route reads the
  same digest with `nix eval`; Windows builds name the commit.
- Checks that compile some packages use `xmtp.filesets.mkClosureSource`. It
  restores the Cargo path-dependency closure of its roots over the dummy
  workspace. `dev = true` adds the roots' dev-dependencies and test data.
  `target = "wasm32-unknown-unknown"` leaves out the dependencies that Cargo
  does not build for that target; a `[target.*]` key with a cfg name that
  `targetCfgs` does not list fails evaluation. In
  `lib/filesets.nix`, add a new data file that a package compiles in to
  `compileData`, a file that `test-utils` code reads to `testUtilsData`, and a
  file that only the package's tests read to `testData`. After a source change,
  show with `nix eval --raw .#<output>.drvPath` that an unrelated edit keeps the
  path and a relevant edit changes it.
- Crane reads `Cargo.lock` and the version from its source during evaluation.
  A derivation that builds from a dummy source passes `version` and
  `cargoVendorDir = xmtp.base.mkCargoVendorDir rust`. Check an output with
  `nix eval --option allow-import-from-derivation false --raw .#<output>.drvPath`.

`services/stack.nix` defines the test services once, as a process-compose
module: a PostgreSQL primary and streaming replica, VersityGW with the
attachments bucket, anvil, the backend, and toxiproxy. A port that the caller
sets wins; every other port is a free loopback port. Commands and probes are
store scripts, because process-compose substitutes `$NAME` in the whole config.
`.#services` runs the stack; `dev/services up|env|ps|logs|down` runs it in the
background on this worktree's ports, without Docker. `.#services-test` starts
the stack, runs `XMTP_SERVICES_TEST_COMMAND`, exits with its status, and stops
every service (`dev/nix-shell 'just backend ci COMMAND'`).
`legacyPackages.mkServices { backend; replica; }` gives these outputs for
another backend binary, so a test derivation keys on the binary that it starts;
without `backend` it is the storage subset (PostgreSQL, replica, S3). Its
`checkCommand` runs a
check phase through `services-test`; the derivation merges its `sandbox`
attributes (Darwin loopback networking, and `sendfile` for VersityGW, with no
other host reachable). A Darwin build shares the host loopback, so a sandboxed
check can reach any service that listens there, such as a developer's Docker
stack or a localhost system proxy; remote hosts stay blocked. A Linux build has
its own loopback. `checks.<system>.services` proves each service inside a
build.

The CI client tests match trunk's CI backend, whose replica URL was the
database URL: `legacyPackages.clientServices` (`replica = false`) runs no
replica and points `XMTP_REPLICA_URL` at the primary. `nextest`, the WASM
partitions, and `.#services-test` (so every `just backend ci` of the iOS,
Android, and JS jobs) use it. `backend-tests` (it tests replica behavior),
`dev/services` (it matches the local Compose stack), and
`checks.<system>.services` keep the streaming replica. A separate change can
give the client tests real replica lag once the tests that sync once after
another client's write poll instead (about 36 Rust call sites).

`nextest.<system>` runs the workspace tests with `clientServices` inside the
sandbox, on free ports.

`wasm-nextest-archive.<system>` compiles the WASM tests into one nextest
archive. `wasm-nextest-part-<n>.<system>` runs slice `n` of 4 from it, with
`clientServices` started inside the build, so it needs no
Docker stack. WASM bakes the service URLs in at compile time, so the services
use the fixed port block 20000-20099, and each slice holds an exclusive lock
on `/tmp/xmtp-wasm-nextest.lock` (a second slice waits). A backend change
reruns the slices and substitutes the archive. The slices still run outside
the sandbox (`__noChroot`, for Chrome). Run them with `just wasm nix-test`
(all slices, one after another) or `just wasm nix-test-part <n>`.

`backend-tests.<system>` runs the backend tests and the SQLx metadata check in
the build sandbox, through `services-test` of the storage services (a
PostgreSQL primary, a streaming replica, and VersityGW on free loopback
ports). Linux runs all tests. Darwin skips the tests that need the system TLS
trust service or fast replica replay. Build it with `nix build --no-link .#backend-tests.<system>`.
`test-backend.yml` builds it on Linux and, in the `Native backend contract`
job, on macOS.

## Bindgen fork pin

The `ubrn-src` flake input is the bindgen fork. Evaluation reads its
`Cargo.lock` with no build. `nix/lib/packages/ubrn.nix` fails evaluation
unless the input revision equals `Cargo.toml [workspace.metadata.xmtp-sdk-fork]`
and `Cargo.lock` takes `ubrn_bindgen`, `ubrn_common`, and `uniffi-runtime-wasm`
from the fork at that revision. To bump the fork:

1. Set the new revision in `Cargo.toml` (the three dependencies and
   `[workspace.metadata.xmtp-sdk-fork]`), and update `Cargo.lock`.
2. Set the same revision in the `ubrn-src` URL in `flake.nix`, and run
   `dev/nix-shell 'nix flake lock'` (or `nix flake update ubrn-src`). Do not use
   the deprecated `--update-input`; it changes nothing for a revision-pinned URL.
3. Update the `npmDepsHash` values in `nix/lib/packages/ubrn.nix` if the fork's
   npm locks changed.
4. Check with
   `nix eval --option allow-import-from-derivation false --raw .#ubjs-node.drvPath`.
5. Commit `flake.nix`, `flake.lock`, `Cargo.toml`, and `Cargo.lock` together.

## Generated SDK preparation

`xmtp-sdk-generated` includes the native, worker, and pure roots with matched
contract records. Select `xmtp-sdk-generated-swift`, `xmtp-sdk-generated-kotlin`,
or `xmtp-sdk-generated-node` to build one native language without WASM.
`xmtp-sdk-generated-browser` includes matched worker and pure roots.
Each selected product keeps the aggregate's root names. Node and Browser
include their required runtime links. Android and Apple preparation use the
selected Kotlin and Swift products. Check these build closures with
`dev/nix-shell 'just sdk check-generated-nix'`.
`xmtp-sdk-pure-wasm` is a separate artifact and shares the
worker dependency cache. The generated library outputs are
`xmtp-sdk-node-<platform>` and `xmtp-sdk-android-<abi>`. Darwin adds `xmtp-sdk-ios-device` and
`xmtp-sdk-ios-simulator`. Public SDK packaging stages these matched generated
artifacts. Building and staging do not publish a package.
Each system builds one host SDK library (`xmtp-sdk-native.nix` with the host
package set; `xmtp-sdk-libs`). The renders, `xmtp-sdk-node-<host>`, the macOS
slice, and the JNA test host load it. On Linux it is the zigbuild build for the
glibc floor. `nix/sdk-platforms.json` is the one list of Node platforms,
Android ABIs, Apple slices, and the glibc floor; Nix, the Node platform
scripts, and `build-sdk-node-platforms.yml` read it.

Run `dev/nix-shell 'python3 -B nix/check-sdk-products.py'` to build selected
Kotlin, Android fast, and Swift on Darwin. It checks generated files, complete
receipts, source identities, and the selected Android ELF library.
Run `dev/nix-shell 'python3 -B nix/test-sdk-windows-staging.py'` to check the
Windows Node job with cached generated roots. This fixture checks cleanup and
receipt creation. It does not compile a Windows SDK.

`android-sdk-libs` combines generated Kotlin, runtime and Android sources,
the contract record, and all four `libxmtp_sdk.so` ABIs. `android-sdk-libs-fast`
selects the host emulator ABI. The SDK Gradle build reads this layout. The
mobile-stage command builds the SDK library AAR so platform helpers are included.

`example-android-apk` builds the signed universal Messenger testing APK on
macOS ARM64 and Linux x86_64. It uses the SDK version in `gradle.properties`, the
four JNI libraries, a pinned Gradle distribution, and the public AOSP testing
key under the separate `org.xmtp.android.example.testing` app ID. It builds the
non-debuggable release variant with an HTTPS placeholder; testers must choose a
server. The build has an empty Gradle home and a local replay proxy for hash-pinned
downloads; it does not access external repositories. The Android build toolchain
omits emulators and system images. Keep its source filter limited to APK inputs.

After APK dependency or plugin changes, run
`dev/nix-shell 'just example-android apk-update-deps'` to update the Gradle download
list. This route uses strict Gradle verification and a fresh cache. Verify the
result with `nix build --no-link .#example-android-apk`. Keep the Nix Gradle pin
matched to the shared wrapper. Private signing keys must stay outside derivations.

The Android minimum-platform CI caller sets `NIX_ANDROID_EMULATOR_API=23`.
Only Linux x86_64 includes this default x86_64 system image. Other callers use
API 34. The launcher checks the selected API and runs the existing clock sync.
Check the selector with `dev/nix-shell 'python3.11 nix/lib/test_android_emulator_platform.py'`.
Check startup supervision with `dev/nix-shell 'python3 dev/tests/test_android_emulator_start.py'`.
The launcher has a five-minute deadline shared by ADB connection, boot, clock
sync, and API verification, and fails when the emulator exits. CI retains the
startup log, AVD configuration, and Crashpad database in its Android artifacts.
`NIX_ANDROID_EMULATOR_LOG_DIR` selects CI's artifact directory; local runs retain
diagnostics under the temporary Android user home printed by the launcher.
`run-test-emulator -- COMMAND ARGS...` owns the emulator through that command,
exports its selected `ANDROID_SERIAL` to the command, and stops its process
group and removes its newly created Android home on success, failure, or TERM/INT.
Scoped-run diagnostics stay outside that home. Calling `run-test-emulator`
without a command leaves the ready emulator and home available for interactive reuse.
