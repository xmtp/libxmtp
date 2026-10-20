# Nix

Check an affected output with `nix build --no-link .#<output>`; run `dev/nix-shell 'just lint-config'`
and `dev/nix-shell 'just test dev'`.

Rust-capable development shells set pinned Kache as `RUSTC_WRAPPER` and point
`KACHE_HOST_CONFIG` at `lib/kache.toml` (`rustShellEnv` in
`lib/shell-common.nix`). Check the package with `nix build --no-link .#kache`
and the effective settings with `dev/nix-shell 'kache doctor'`.
CI adds its settings through `setup-nix`. Nix build derivations retain their
Nix/Crane caches; they do not receive the outer shell's compiler cache.
Keep compiler credentials and writable host cache paths out of derivations.

On macOS, keep the compiler, linker, and SDK from the same toolchain. The local
and iOS shells select Xcode through `ios-env.nix` and use its native linker.
Do not use Nix's linker with the system SDK. The shells accept any Xcode. The
iOS build outputs are keyed to one Xcode (`xcode` in `ios-env.nix`, also
`lib.xcode`): a CI or release output fails to build with another, and the
`-local` outputs (`ios-xcframeworks-local`, `ios-xcframeworks-fast-local`)
build with any Xcode. Canonical outputs select the keyed absolute developer
directory per process and check its exact version, build, and path. They do
not depend on the host's global selection. Keep the CI installation pins
matched when changing `xcode`. RN has the separate pin described below.
The focused Rust shell and Android host builds use the Nix toolchain.
They must replace native compiler and linker overrides from an outer local or
iOS shell. Otherwise, Xcode's linker uses Nix's SDK without its library search paths.
Run `dev/nix-shell 'dev/check-apple-toolchain'` in the local shell and
`NIX_DEVSHELL=rust dev/nix-shell 'dev/check-apple-toolchain'` for the Nix SDK.
Also check a shell transition with
`dev/nix-shell 'dev/nix-shell --shell rust "dev/check-apple-toolchain"'`.
Also check the Android shell transition with
`dev/nix-shell 'dev/nix-shell --shell android "dev/check-apple-toolchain"'`.
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
- Apple SDK builds use one codegen unit for `xmtp_db` through the same Cargo
  package profile in dependency and final builds. Its generated migration
  paths shift rustc inline assembly span metadata, which ThinLTO hashes into
  native symbol suffixes. Keep this workaround scoped to Apple SDK builds;
  removing it needs independent native archive and dylib byte comparisons.
  Final Apple SDK fixup uses LLVM ranlib: cctools ranlib records the build
  user's UID when Nix rebuilds stripped archive indexes.
- Backend builds restore only the shared dependency sources over the dummy workspace. Include migrations, SQL query files, and `apps/backend/.sqlx` metadata. Add each new shared dependency to the restored crate list. Build with `SQLX_OFFLINE=true`; no database is allowed during the build.
- The Rust shell provides sqlx-cli. Keep its version aligned with the backend SQLx dependency.
- SDK compiler sources use a dummy workspace and restore the selected local
  dependency graph. Keep embedded SDK data and bindgen templates in the real
  compiler source. Provenance is an evaluation-time digest
  (`lib/sdk-sources.nix`, `.#sdkProvenance`): `source` of the SDK closure,
  `generator` of the bindgen closure, and `render.<swift|kotlin|typescript>` of
  the bindgen closure plus that language's runtime and configuration. A tree's
  receipt records its language's `render` identity, so a Kotlin runtime edit
  changes only the Kotlin trees. Receipts name files by name, so a product references no source and
  keeps its path on a commit outside the closure. The cargo route reads the
  same digest with `nix eval`; Windows builds name the commit and a digest of
  uncommitted SDK input changes. The release Windows Node pair is cached by
  `sdk-artifacts.py key` (the committed SDK input trees, profile, target,
  fork revision, the committed node-platforms workflow, and the compiler); its receipt names that source key and the commit that built
  it, and staging checks the key, not the commit.
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
`test-nix-checks.yml` builds it on Linux. No CI job builds the Darwin
variant; `test_ios` runs the backend and the test services on macOS.

## Bindgen fork pin

The `ubrn-src` flake input is the bindgen fork. Evaluation reads its
`Cargo.lock` with no build. `nix/lib/packages/ubrn.nix` fails evaluation
unless the input revision equals `Cargo.toml [workspace.metadata.xmtp-sdk-fork]`
and `Cargo.lock` takes `ubrn_bindgen`, `ubrn_common`, and `uniffi-runtime-wasm`
from the fork at that revision. Only outputs that use `xmtp.ubrn` evaluate the
check: `ubjs-*`, the `xmtp-sdk-node-*` packages, and the TypeScript consumers
(`xmtp-sdk-generated`, `-node`, `-browser`, and the WASM test fixtures, whose
trees link the runtimes). `xmtp-sdk-generated-swift` and `-kotlin` do not, and
`just sdk lint` no longer checks the pin. To bump the fork:

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

Each generated root is a copy of one render: `xmtp-sdk-bindgen render` writes
the tree and its receipt, and formats TypeScript with the nixpkgs `oxfmt`.
`xmtp-sdk-generated` includes the native, worker, and pure roots. Select `xmtp-sdk-generated-swift`, `xmtp-sdk-generated-kotlin`,
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

`android-sdk-libs` combines generated Kotlin, runtime and Android sources,
the contract record, and all four `libxmtp_sdk.so` ABIs. `android-sdk-libs-fast`
selects the host emulator ABI. The SDK Gradle build reads this layout. The
mobile-stage command assembles the SDK library AAR over the per-ABI libraries
(`xmtp-sdk-android-<abi>`), so platform helpers are included.

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

## React Native

RN runtime sources (`ubjs-react-native`) come from the same fork as bindgen.
The RN render has its own `sdkProvenance.render.react-native` identity. Native
adapters and package tools stay outside the Rust compile closure. The JS/types
product `xmtp-sdk-react-native-js` has no Android/iOS binary dependency; docs
use it directly.

`dev/nix-shell --shell react-native 'COMMAND'` selects the locked RN/Expo
Android tools, Gradle, JDK, and CocoaPods. It does not change sibling pins.
`lib.reactNativeXcode` and `lib/react-native-env.nix` own RN's separate exact
Apple pin. Keep the sibling Xcode in `ios-env.nix`. Each canonical derivation
selects its own pin, so a cold component build needs both installations but
needs no global Xcode switch or separate native-base build.

Apple SDK compiles remap transient build roots through the stable compiler
wrappers in `lib/apple-native-tool.sh`. Dependency and final phases use the
same wrapper paths and caller flags, so remapping does not invalidate Cargo's
dependency fingerprints. The C wrapper also stabilizes OpenSSL's configured
engine/module directory macros before compilation. It does not rewrite
sources or completed artifacts. Exact NAR and signature checks still admit
only the tested bytes; remapping alone is not proof of byte determinism.

The shared CI installer admits exact read-only host inputs through
`extra-allowed-impure-host-deps`: the pinned Xcode Contents directory,
`/usr/bin/xcrun`, `/usr/bin/ditto`, `/usr/bin/env`, `/usr/bin/touch`, the existing Xcode license record at
`/Library/Preferences/com.apple.dt.Xcode.plist`, and `/usr/share/firmlinks`.
The launcher checks the license record; Xcode SDK discovery reads firmlinks.
Stock Expo header phases use `ditto`; their unchanged shell scripts use `env`
to select the declared Nix Bash or Node from the existing build PATH. Xcode uses
its hardcoded `touch` executable to finalize frameworks and app bundles.
Simulator and device plug-ins also load the exact CoreSimulator, DVTDownloads,
ROCKit, CoreDevice, and CoreDeviceUtilities bundles under
`/Library/Developer/PrivateFrameworks`, and MobileDevice and Mercury under
`/Library/Apple/System/Library/PrivateFrameworks`. MobileDevice is the real
target of its `/System/Library/PrivateFrameworks` symlink. These are bundle
reads only; they do not grant service or network access.
These inputs do not accept a license or change host preferences.

Only the Expo iOS Nix consumer selects the guarded CocoaPods adapter. After a
successful real install, it atomically adapts the exact locked ExpoModulesJSI
58.0.15 build script. The nested SwiftPM build uses an owned home and manifest
module cache, the same no-signing intent as the app, and a process-local setting
that disables only SwiftPM's additional manifest sandbox. The outer Nix sandbox
and network denial stay active. The dependency script remains unchanged in the
lockfile and content store; no global defaults are written. After verifying the
locked Expo macro package and executable, the adapter also appends the pinned
Swift compiler's `-Xfrontend -disable-sandbox` only to generated `OTHER_SWIFT_FLAGS`
lines that load that exact macro. This disables the compiler's additional sandbox
for subprocesses in those invocations, including loaded Xcode plug-ins; the outer
Nix sandbox still applies. Existing inherited and module-map flags are preserved.
Only the Expo iOS consumer exports the locked CLI's shell-only
`__UNSAFE_EXPO_HOME_DIRECTORY` override to `$TMPDIR/expo-cli-home`. Xcode can
replace `HOME` with the build user's passwd home; the CLI override keeps its
anonymous ID, settings, and cache writes in the owned build directory without
changing `HOME`, project dotenv files, or host permissions.
The Darwin native
dependency farm retains this adapter and its declared tools.
The two iOS consumer app derivations also allow only the read-only LaunchServices
`com.apple.lsd.mapdb` lookup. It reads installed host type-registration metadata
so Xcode can recognize workspace types. Nix accepts
this additive rule only with command-local `sandbox=relaxed`; these app roots
set `__noChroot=false` and remain sandboxed, including network denial. This is
not a global setting. Native/base/component and fixed-output dependency modes
retain their existing behavior.

Import the checked canonical package before building consumers. All four
reachable native `__noChroot=true` derivations are behind that already-realized
package output, so the consumer step does not build them. CI and release select
the mode only for the Darwin consumer command. Trusted warming builds the
consumers in the post-merge RN cache job, which builds the RN warm set
(`ciSets.<system>.rn`) with the same mode. The dev and release sets hold no
consumer, so their build needs no relaxed sandbox.

On a local trusted Nix installation,
a canonical consumer build can use the same narrow per-command options:
`dev/nix-shell 'nix build --option sandbox relaxed --option extra-allowed-impure-host-deps "/Applications/Xcode_27.0.app/Contents /usr/bin/xcrun /usr/bin/ditto /usr/bin/env /usr/bin/touch /Library/Preferences/com.apple.dt.Xcode.plist /usr/share/firmlinks /Library/Developer/PrivateFrameworks/CoreSimulator.framework /Library/Developer/PrivateFrameworks/DVTDownloads.framework /Library/Developer/PrivateFrameworks/ROCKit.framework /Library/Developer/PrivateFrameworks/CoreDevice.framework /Library/Developer/PrivateFrameworks/CoreDeviceUtilities.framework /Library/Apple/System/Library/PrivateFrameworks/MobileDevice.framework /Library/Apple/System/Library/PrivateFrameworks/Mercury.framework" --no-link .#react-native-consumer-ios'`.
An untrusted daemon user must ask its administrator to admit those exact paths.
Keep the sandbox enabled; do not allow all of `/Applications` or `/usr/bin`. The local shell
route below does not build the canonical consumer derivation.

For local Apple proofs, use `dev/nix-shell --shell react-native-local 'COMMAND'`.
This opt-in shell resolves the selected host Xcode and exports its full identity
with `XMTP_RN_XCODE_MODE=local`. Canonical shells and app derivations require
the exact versioned Xcode path and export mode `canonical`. Both remove Nix's
`SDKROOT` override and route Apple tools to the real Xcode. Local receipts
must keep their local identity; they do not satisfy canonical CI admission.

`react-native-js-tools-deps` retains the four exact compiler/declaration
archives as a narrow warm-set member. Keep it in each producer stage and in a
warm set (dev on Linux, rn on Darwin) so an empty-store closure check also
covers these build-time inputs.

RN Gradle runs on JDK 21. Its included settings plugin compiles with the
declared JDK 17. The Gradle installation properties name both JDKs and disable
automatic discovery and downloads. Both RN shells select this Gradle through
`XMTP_RN_GRADLE`. Keep its closure in the Android native dependency farm.
The iOS native dependency farm retains the declared `rsync` used by CocoaPods.

Linux builds `xmtp-sdk-react-native-android`; Darwin builds
`xmtp-sdk-react-native-ios`. `ciStages.<system>.components` lists the host's
complete producer outputs. The Linux `xmtp-sdk-react-native-package` declares
both immutable component inputs. Export/import them with
`python3.11 nix/react-native-exchange.py` through the RN recipes. Assembly must
follow both producers; consumer bundles follow assembly. PR exports stay in
CI jobs. Trusted writers receive only exports from the same main run.
Jobs with a Cachix token skip the exchange; `react-native-exchange.py roots`
prints a stage's roots. `--kind test` selects
`legacyPackages.<system>.reactNative.testStages`, the stages of the partial
`xmtp-sdk-react-native-test-package`; `--kind test-android` selects
`androidTestStages`, the Linux-only stages of
`xmtp-sdk-react-native-test-package-android` (the Android smoke).

Check export admission with
`dev/nix-shell 'python3.11 -B nix/lib/test_react_native_exchange.py'`.

Expo's published native build scripts use `shasum`; the iOS consumer and RN
Darwin shells declare Nix Perl for it. Keep Perl and rsync in the iOS native
dependency farm so post-merge warming retains these build tools.

RN CocoaPods probes spawn `command -v ccache` directly through Ruby. The
`react-native-command.nix` store tool delegates to the pinned shell builtin.
Keep it in the iOS consumer, Darwin RN shells, and iOS warm tool farm; it
does not require host `/usr/bin/command` or enable ccache.
