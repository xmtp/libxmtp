# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`dev/nix-shell 'just backend status'` to find this worktree's backend ports.

- `dev/nix-shell 'just sdk generate'` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`. In
  each TypeScript tree, `index.ts` is the package root: the public layer that
  the projection generates. The stock UniFFI root is the private `binding.ts`.
  The Node public layer imports it to load the native binding; otherwise only
  the worker and transport tests import it.
- `dev/nix-shell 'just sdk check-package-scripts'` runs the normal packaging controls.
  Android dependency-input cases use `dev/sdk-packaging-android-inputs.py`,
  which the main packaging suite loads as inherited test methods.
  Both packaging suites share the fixture in `dev/packaging_test_base.py`.
- `dev/nix-shell 'just sdk check-native-nix'` evaluates native build inputs and compares
  the checkout source identity with the generated and native Nix source filters.
  It does not compile a product.
- `dev/nix-shell 'just sdk check-generated-nix'` checks the real generated SDK and
  Android fast/full build closures. Native generated products exclude SDK
  WASM builds. Android uses selected Kotlin. Apple uses selected Swift.
  This command does not compile a product.
- `dev/nix-shell 'just sdk check-file-sizes'` checks the 1,000-line limit for every SDK source
  file, including conformance files. Generated and ignored build files are excluded.
  Keep most new files below 500 lines.
- `dev/nix-shell 'just sdk lint'` checks file sizes, generated names, and TypeScript source.
  Run `dev/nix-shell 'just sdk generate'` first. Lint stops when a generated target root is missing.
  It also rejects test-only hooks (`*ForTest`, `*_for_test`, `bridge_test_panic`) in
  the default bindings and in `apps/xmtp_sdk_bindgen/runtime/`. Keep test hooks
  in test source sets.
- `dev/nix-shell 'just sdk wasm-init'` loads the staged WASM package in Node.
- `dev/nix-shell 'just sdk conformance <swift|kotlin|node>'` runs scenarios against this
  worktree's backend. `dev/nix-shell 'just sdk conformance browser'` runs Chromium
  proofs in Vitest Playwright: a real WASM trap from a test-only panic fixture,
  storage layouts, attachment and event lifetime, and decode-once. It then
  checks real OPFS and worker behavior. The public browser scenarios are in
  `sdks/browser/test`. Its recipe builds the
  pure codec and panic fixtures in the Rust shell before the JS shell.
  Kotlin JVM conformance uses small Android platform stand-ins for the storage
  helper and cleaner. It selects the JNA cleaner branch. Installed Android tests
  use the platform classes.
  Scenario 7 checks readers and streams. Scenario 8 checks events and listeners.
  The browser attachment worker-death proof uses the generated public package
  and its shared worker manager.
  The Swift run has no scenarios. It checks the missing bundle identifier in a
  bare executable, the reader and listener proofs that need the injected
  runtime seams, and the negative consumers. The other Swift checks are in
  `sdks/ios/Tests`. Swift CI uses `dev/nix-shell 'just sdk generate swift'`, then
  `dev/nix-shell 'just backend ci just sdk conformance swift'`.
  The Kotlin, Node, and browser runs start `conformance/ts/object-store.mjs` for their
  download fixtures. The default ephemeral fixture port keeps `SDK_FIXTURE_URL`
  separate from native S3 on port 9067. Each run sets `SDK_RELAY_TARGET` to the backend. The fixture can hold a
  small PUT response, count upload grants and object requests, and refuse
  selected relayed backend URLs. It uses `protoc` from the Rust shell to
  replace only the upload URL in a real backend response. Native clients use
  the fixture's HTTP/2 relay; browser clients use its gRPC-web relay. Both
  preserve gRPC status trailers.
- `dev/nix-shell 'just sdk bench <node|browser|swift|kotlin> [--samples N]'` measures
  the staged package on one host against this worktree's backend and writes
  `results.json` (p50 and p95, no pass or fail). Stage the package first. For
  browser, swift and kotlin, set `NIX_DEVSHELL=js`, `ios` or `android` inside
  the command, for example `dev/nix-shell 'NIX_DEVSHELL=js just sdk bench browser'`.
  See `benchmarks/README.md`. CI does not run it.
  The run accepts only a loopback `XMTP_BACKEND_URL` (`localhost`, `127.0.0.1`,
  `::1`). Each run publishes messages that the backend keeps. For another host,
  pass `--allow-remote-backend`; the run then prints a warning and takes at
  most 3 samples per workload.
- `dev/nix-shell 'just sdk bench-check'` runs the benchmark unit tests and static runner
  checks. It needs no backend, device or SDK build.
- `dev/nix-shell 'just sdk conformance-bridge'` runs bridge Vitest, real WASM worker proofs,
  and Chromium proofs for pure codecs, worker failure, and browser storage.
  It also checks the public log setter, the real Rust queue, and final managed
  worker retirement with held app callbacks.
- `dev/nix-shell 'just sdk conformance-storage'` runs the real-worker OPFS proof against the
  staged SDK. Run `dev/nix-shell 'just sdk generate'` first after SDK or runtime changes.
- `dev/nix-shell 'just sdk conformance-package'` checks package creation reservations, shared
  client/admin workers, final worker termination, and collection in Chromium.
  Run `dev/nix-shell 'just sdk generate'` first after SDK or runtime changes.
- `dev/nix-shell 'just sdk conformance-bridge-unit <vitest arguments>'` runs focused bridge
  unit tests against the staged SDK.
- `dev/nix-shell 'just test crate xmtp_sdk'` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.

Content and conversation exports use named `include!` files to keep UniFFI
module paths stable. Use ordinary modules for helpers without exported metadata.

## Adding exports

Export impl blocks, traits, and functions with `#[xmtp_macro::sdk_export]`.
A record or enum that needs a marker takes it too, as its first attribute,
above every derive. The generator reads what it needs from the macro's
metadata markers, so a routine export needs no generator edit:

- A synchronous getter takes `#[sdk(immutable)]` when its value never changes
  for the object's lifetime. Otherwise make it async. The crate does not
  compile with an unmarked getter that the browser bridge forwards.
- Limit an exported item to native targets with `#[sdk_export(native_only)]`;
  it is the same `#[cfg]`.
- A new event takes an `EventKind` variant with
  `#[sdk(kind = "namespace.name")]` and a `ClientEvent` variant of the same
  name. Its payload records keep their Rust field names, and its enums use
  snake_case values, in TypeScript. Generation stops when another call
  shares such a record, or such an enum with a multi-word value.
- A record or variant field whose value must stay out of diagnostic text
  takes `#[sdk(redact)]`, or `#[sdk(redact = "key")]` for one key of a string
  map. Every other field of that record or variant then takes
  `#[sdk(redact)]` or `#[sdk(shown)]`, and the type writes an `impl Debug`
  that redacts the same fields instead of deriving one. Keep
  `#[xmtp_macro::sdk_export]` the first attribute: the macro cannot see a
  `#[derive(Debug)]` written above it.
- A `MessageData` field, a `*_with_backend` `Client` static, or a new identity
  route still needs the hand edits that the generator README lists.

`apps/xmtp_sdk_bindgen/README.md` lists the markers and the areas that stay
hand-maintained.

## Matched package preparation

- `dev/nix-shell 'just sdk build [swift,kotlin,node,browser]'` builds selected artifacts once.
  It records file hashes under `target/sdk-artifacts/`. Native targets do not
  build WASM. Full and pure WASM use separate output directories.
- `dev/nix-shell 'just sdk render [swift,kotlin,node,browser]'` uses those artifacts. It deletes
  and regenerates each selected target and does not change other targets.
  It first checks the artifact bytes against `artifacts.json`. For `node` and
  `browser` it also rejects artifacts from older Rust or generator source. Swift
  and Kotlin output is checked later by mobile preflight and Swift conformance
  staging. Run `build` first after a Rust or generator change.
  Package staging requires exact generated asset sets and hashes. Unlisted
  generated files fail before runtime or compiler work.
  `dev/nix-shell 'just sdk generate [targets]'` runs both steps. Use `--profile release` on the
  build recipe for release proofs. Conformance shares the bindgen artifact.
- `dev/nix-shell 'just sdk check-package-scripts'` checks reuse, toolchain inputs, and cleanup
  with a small fixture. `dev/nix-shell 'just sdk check-clean-generate'` adds one real Swift
  render. It uses existing artifacts and does not rebuild Rust.
  The script checks also reject missing or extra pure WASM functions and pure
  functions in worker bindings or dispatch. The exact set comes from approved
  pure Rust declarations in the current isolated SDK source.
- `dev/nix-shell 'just sdk stage node'` and `dev/nix-shell 'just sdk stage browser'` compile ESM products with
  tsdown. They copy the pinned runtimes, native library, worker, pure WASM,
  loaders, and snippets. `dev/nix-shell 'just sdk package-smoke node|browser'` packs each product
  and installs it in an empty consumer. It checks a codec round trip without
  package receipts or runtime asset hashes. Browser smoke also loads its worker.
  Package configuration and asset identity are checked during staging and tests.
  Switched SDK package builds use `bash ../../dev/js/sdk-package node|browser`
  from the SDK directory. The helper stages a public manifest and copies the
  complete product into the SDK's `dist` directory for workspace imports.
  Release jobs pack `target/sdk-packages/<target>` directly. Private staging
  remains the default for conformance.
- Use `NIX_DEVSHELL=ios dev/nix-shell 'just sdk mobile-build ios'` for the iOS
  device and simulator libraries. Then use the same shell for
  `dev/nix-shell 'just sdk mobile-stage ios'` to assemble `XmtpSdkFFI.xcframework` and SwiftPM
  sources. These commands require Xcode.
- Use `NIX_DEVSHELL=android dev/nix-shell 'just sdk mobile-build android'` for
  arm64-v8a, armeabi-v7a, x86_64, and x86. Then use the same shell for
  `dev/nix-shell 'just sdk mobile-stage android'` to assemble the AAR. Its Kotlin compiler uses
  `-Xjvm-default=all`. Final device and emulator runtime proofs use the matched
  products. A source-only consumer does not replace those proofs.
- Private proof inputs can use `XMTP_SDK_GENERATED_DIR` and
  `XMTP_SDK_PACKAGES_DIR`. Prebuilt runtime inputs can use
  `XMTP_SDK_RUNTIME_DIR` (a directory with core/node or core/wasm products).
  These variables configure build tools. They add no SDK runtime option.

The old SDK packages, binding outputs, release jobs, and version numbers stay
in place until their owning Phase 2 switches. New package preparation does not
publish a product. All switched SDKs will use the approved 8.0.0 version line.

Package review checks:

- `dev/nix-shell 'just sdk check-package-scripts'` also checks both provenance producers,
  config-only changes, Cargo compiler overrides, macOS deployment targets,
  Windows browser asset paths, default mobile features, all four NDK compiler targets and archive index tools,
  caller archive-tool policies, target OpenSSL paths and policy, cache inputs, and both flat and prebuilt runtime
  directory layouts.
  Explicit target OpenSSL paths keep upstream library-directory selection.
  Generic host roots, library paths and header paths use the selected compiler's
  host-qualified variables in the Android and iOS child environments. A policy-only
  override keeps generic paths. Both mobile routes build target
  OpenSSL by default. Explicit target paths and policies stay intact. Parent
  inputs stay intact.
- Use `NIX_DEVSHELL=android dev/nix-shell 'just sdk check-android-toolchain'`
  for small C probes. The output records ELF class and machine for each ABI.
  These probes do not prove an installed Android SDK.
- The private compiler input is `XMTP_SDK_TSDOWN_CLI`. It names tsdown's
  `dist/run.mjs`, which the stager runs through Node on every platform.
  Windows CI stages with a supported compiler Node version and runs the
  installed smoke on the minimum SDK Node version, 22.12.0.

Residual package review checks:

- Build provenance includes the live address registry, chain URL map, and
  signature validation bytecode. The common receipt producer uses the same
  fingerprint. Mobile preflight requires each native receipt's exact triple.
- Installed smoke runs `npm-cli.js` through Node. `XMTP_SDK_NPM_CLI` is a
  private path override for the launcher proof. The normal path comes from
  the selected Node installation, including the Windows installation.

- Use `dev/nix-shell 'just sdk check-native-nix'` to check the evaluated SDK
  build inputs. Cargo uses its normal or caller-selected job count. Native stages use
  vendored static OpenSSL. Apple stages keep macOS 11 and iOS 14 floors.
  `just sdk check-package-scripts` checks this gate under Python optimization
  with invalid inputs for each build stage.
  This check does not prove archive linkage or installed package loading.
Android staging dependency inputs:

- The staging Gradle root is `crates/xmtp_sdk/packaging/android`. Keep its
  `gradle.lockfile`, `buildscript-gradle.lockfile`, and
  `gradle/verification-metadata.xml` with the staging source.
- The normal stage command uses strict verification and does not write inputs.
  To refresh inputs, use a separate controlled resolution through
  `NIX_DEVSHELL=android dev/nix-shell`. Resolve the actual `assembleRelease`
  route with `--write-locks --write-verification-metadata sha256`. Review the resolved graph, repositories, and SHA256
  entries before acceptance. Keep metadata verification enabled.
  After refresh, keep each verification `<component>` on one line. Keep all
  checksum values and policy entries. This keeps the generated inventory within
  the SDK file-size limit.
- Record the actual plugin classpath and each resolved release configuration.
  AAR output hashes do not prove dependency input coverage.
- The switched Android project owns its own graph under Task 14. Do not copy
  staging lock state into a different Gradle root.

- The switched Android stage builds `sdks/android/:library:assembleRelease`.
  It checks that selected SDK root's `buildscript-gradle.lockfile`,
  `library/gradle.lockfile`, and `gradle/verification-metadata.xml`.
  The staging fixture inputs do not cover this graph. An explicit
  `--sdk-root` selects the root whose inputs and output are used.
