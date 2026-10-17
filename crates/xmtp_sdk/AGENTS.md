# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`dev/nix-shell 'just backend status'` to find this worktree's backend ports.

- `dev/nix-shell 'just sdk generate'` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`.
  `XMTP_SDK_BUILD` picks the route: `cargo` (the default; debug unless
  `--profile release`) or `nix`, which copies the release-profile
  `.#xmtp-sdk-generated-<target>` trees (out-links in `target/sdk-generated.nix`).
  CI JavaScript jobs and the docs site use `nix`; use it locally too for work
  that edits no SDK crate (a Cachix fetch and a copy). The cargo route takes
  the bindgen from `nix build .#xmtp-sdk-bindgen` unless a file of the
  bindgen's crates (`dev/sdk-artifacts.py paths xmtp-sdk-bindgen`) or a Cargo
  input differs from the merge base with `origin/main`; then it builds
  a cargo debug bindgen. CI keeps cargo. `XMTP_SDK_BINDGEN=nix|cargo`
  overrides the choice. A browser-only build skips the host library, and the
  two WASM builds share one target directory. For Swift and Kotlin unit tests
  after a Rust edit, `just ios test-host` and `just android test-host` use a
  cargo debug host library and its render (`dev/host-package`), with no Nix
  release build. Use the release route (`just ios test`, `just android
  test`, simulators, emulators) before a merge: CI runs it, and the release
  libraries abort on panic where debug ones unwind. In
  each TypeScript tree, `index.ts` is the package root: the public layer that
  the projection generates. The stock UniFFI root is the private `binding.ts`.
  The Node public layer imports it to load the native binding; otherwise only
  the worker and transport tests import it.
- `dev/nix-shell 'just sdk check-package-scripts'` runs every `dev/test_*.py`
  (unittest discovery) and every `dev/test-*.mjs` (`node --test`); a new test
  file runs without a recipe edit. The packaging suites share the fixture and
  the Android dependency-input cases in `dev/packaging_test_base.py`.
- Evaluation checks the native build inputs: `base.assertNativeEnv`
  (`nix/lib/base.nix`) fails evaluation of the host, Android, and iOS
  libraries, or of their dependency layers, without static vendored OpenSSL,
  the macOS 11.0 floor, or the iOS 14 export.
- SDK receipts record the closure identities that Nix computes at evaluation
  (`nix eval .#sdkProvenance`). `dev/sdk-artifacts.py` reads them with the same
  command, so both routes record the same values. Nix evaluates the git
  checkout: `git add` a new file before a cargo build records it.
- `xmtp-sdk-bindgen render` writes each generated tree and its receipt in both
  routes (`apps/xmtp_sdk_bindgen/README.md`). It formats TypeScript with the
  nixpkgs `oxfmt`: the `default` shell exports `XMTP_SDK_OXFMT`, and in the
  `rust` shell `dev/sdk-artifacts.py` builds `nixpkgs#oxfmt` from this
  checkout's `flake.lock`. Generation needs no `pnpm install`.
- `dev/nix-shell 'just sdk check-generated-nix'` checks the real generated SDK and
  Android fast/full build closures. Native generated products exclude SDK
  WASM builds. Android uses selected Kotlin. Apple uses selected Swift.
  This command does not compile a product.
- `dev/nix-shell 'just sdk check-file-sizes'` checks the 1,000-line limit for every SDK source
  file. Generated and ignored build files are excluded.
  Keep most new files below 500 lines.
- `dev/nix-shell 'just sdk lint'` runs `lint-source` and `lint-generated`.
  `lint-source` needs no generated tree: file sizes, the test-hook self-test,
  public names in the sources, and the runtime and template formats; CI runs
  it before generating. `lint-generated` runs the table-driven
  `dev/check-generated-surface`, test hooks, Oxlint (which forbids type
  assertions in the bridge; `as const` stays allowed), and two `tsc` calls
  (the pure entry and the browser proofs get no Node types). Run
  `dev/nix-shell 'just sdk generate'` first. It stops when a generated target root is missing.
  The TypeScript runtime imports the binding, so the lint and the bridge tests
  read its copy in each generated tree; `apps/xmtp_sdk_bindgen/GENERATED_LINT.md`
  has the details. Each generated TypeScript tree links its `@ubjs` packages
  in its own `node_modules`, in both the Nix and cargo routes. For editors,
  `apps/xmtp_sdk_bindgen/runtime/ts/tsconfig.json` resolves the binding in the
  generated Node tree.
  It also rejects test-only hooks (`*ForTest`, `*_for_test`, `bridge_test_panic`) in
  every `.swift`, `.kt`, and `.ts` file of the default trees and in
  `apps/xmtp_sdk_bindgen/runtime/`. The scan skips test sources (`*.test.*`,
  `tests/`, `src/test/`, `Tests/`), such as the generated
  `conformance.gen.test.ts`. Keep test hooks in test source sets.
- `dev/nix-shell 'just sdk wasm-init'` loads the staged WASM package in Node.
- `dev/nix-shell 'just sdk test-bridge [vitest arguments]'` runs the bridge
  runtime unit tests in `apps/xmtp_sdk_bindgen/runtime-tests/ts` and the
  generated `conformance.gen.test.ts` against the staged SDK. The generated test
  also runs the real WASM worker proofs `bridge.real.mts` and
  `bridge.values.real.mts` against this worktree's backend.
- `dev/nix-shell 'just sdk test-browser'` runs the browser platform proofs in
  `sdks/browser/test/platform`. It runs the real WASM worker proofs in Node
  (panic, garbage collection, and the log queue), the Chromium proofs (pure
  codecs, worker failure, late reader opening, storage and OPFS, package
  workers, and logging), and then the Vitest Playwright suite (a real WASM
  trap, storage layouts, attachment and event lifetime, and decode-once). The
  recipe copies the pure codec and panic fixtures from Nix
  (`.#xmtp-sdk-pure-fixture-codec`, `.#xmtp-sdk-wasm-fixture-panic`) with
  `dev/copy-browser-fixtures`. These release-profile fixtures share the
  product WASM dependency layer and enable the test-only `conformance` cargo
  feature, whose exports
  (`sdk_conformance_*`) the default bindings omit. The public browser scenarios
  are in `sdks/browser/test`.
  The run starts `sdks/browser/test/platform/object-store/object-store.mjs`
  (`with-object-store` beside it) for its download fixtures. The default ephemeral
  fixture port keeps `SDK_FIXTURE_URL` separate from native S3 on port 9067.
  The run sets `SDK_RELAY_TARGET` to the backend. The fixture can hold a small
  PUT response, count upload grants and object requests, and refuse selected
  relayed backend URLs. It uses `protoc` from the js shell to replace only
  the upload URL in a real backend response. Browser clients use its gRPC-web
  relay, which preserves gRPC status trailers. The Node tests in
  `sdks/node/test` do not use this fixture. The attachment end test starts its
  own held-upload relay (`sdks/node/test/heldUpload.ts`) on ephemeral loopback
  ports. It needs no `SDK_FIXTURE_URL` and no `protoc`.
- `dev/nix-shell 'just sdk test-browser-logging'` runs only the real WASM worker
  and Chromium log queue proofs. It copies the panic fixture again and
  checks held callbacks, overflow, replacement, and delayed packet admission.
  Run `dev/nix-shell 'just sdk generate node,browser'` first.
- `dev/nix-shell 'just sdk test-browser-storage'` runs only the real-worker OPFS
  proof, and `dev/nix-shell 'just sdk test-browser-package'` runs only the
  package worker proof (creation reservations, shared client/admin workers,
  final worker termination, and collection). Run
  `dev/nix-shell 'just sdk generate'` first after SDK or runtime changes.
- The Swift reader and listener proofs are XCTest cases in `sdks/ios/Tests`
  (`ReaderTeardownTests`, `ListenerGateTests`).
  `dev/nix-shell 'just backend ci just ios test-lifecycle'` runs only them.
  `dev/nix-shell 'just ios check-consumer'` runs the missing bundle identifier
  check in a bare executable and the Swift negative consumers.
  `dev/nix-shell 'just android check-consumers'` runs the Kotlin negative
  consumers. Swift checks run in `test-ios.yml`. Kotlin consumer
  checks run in `test-android.yml`. Selected suites gate aggregate `Test`.
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
- `dev/nix-shell 'just test crate xmtp_sdk'` runs the façade tests against the local backend.
  The handle tests that claim more than once per inbox, or need another
  handle setting, also start a disposable backend (`xmtp_backend`'s
  `TestServer`). It creates its own database on this worktree's `db` service.

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
  map. Every other field of that record, or of any variant of that enum,
  then takes `#[sdk(redact)]` or `#[sdk(shown)]`. The macro implements `Debug` for the
  type through its `fn redacted_debug(&self, f)`, which redacts the same
  fields, so the type cannot also derive `Debug`.
- A backend function that the Client also exposes takes
  `#[sdk_export(client_static)]`; it must be async and available on every
  target. Every SDK gets a Client static named without the trailing
  `_with_backend`, with the `BackendSource` argument last, and the function
  stays exported.
- A `MessageData` field gets an accessor on `Message` in every SDK, and a
  place in Kotlin's `Message` equality, from the generator.
- A new identity route still needs the hand edits that the generator README
  lists.

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
  and Kotlin output is checked later by mobile preflight. Run `build` first
  after a Rust or generator change.
  Package staging requires exact generated asset sets and hashes. Unlisted
  generated files fail before runtime or compiler work.
  `dev/nix-shell 'just sdk generate [targets]'` runs both steps. Use `--profile release` on the
  build recipe for release proofs.
- `dev/nix-shell 'just sdk check-package-scripts'` checks reuse, toolchain inputs, and cleanup
  with a small fixture, including stale-file cleanup on render.
  The script checks also reject missing or extra pure WASM functions and pure
  functions in worker bindings or dispatch. The exact set comes from approved
  pure Rust declarations in the current isolated SDK source.
- `dev/nix-shell 'just sdk stage node'` and `dev/nix-shell 'just sdk stage browser'` compile ESM products with
  tsdown. They copy the pinned runtimes, native library, worker, pure WASM,
  loaders, and snippets. `dev/nix-shell 'just sdk package-smoke node|browser'` packs each product
  and installs it in an empty consumer. It checks a codec round trip without
  package receipts or runtime asset hashes. Browser smoke also loads its worker.
  Package configuration and asset identity are checked during staging and tests.
  SDK package builds use `../../dev/js/sdk-package node|browser`
  from the SDK directory. The helper stages a public manifest and copies the
  complete product into the SDK's `dist` directory for workspace imports.
  Release jobs pack `target/sdk-packages/<target>` directly. Private staging
  remains the default for tests.
- The Android and iOS native libraries have one producer, Nix. `dev/nix-shell
  'just sdk mobile-stage ios'` copies the release `.#ios-xcframeworks` into a
  SwiftPM package after checking its slices against `nix/sdk-platforms.json`.
  It selects the output with `sdks/ios/dev/bindings --release --build-only`:
  the CI output when this Xcode matches or a cache holds it, else the
  `-local` output (an error in CI).
  `XMTP_SDK_BUILD=nix just sdk generate kotlin`, then `just sdk mobile-stage
  android`, assembles the AAR over the Nix JNI library of each ABI
  (`.#xmtp-sdk-android-<abi>`) after checking each library's receipt against
  the Kotlin binding. The Kotlin compiler uses `-Xjvm-default=all`. The SDK
  benchmarks read these packages; CI stages the AAR in the Android SDK build
  job. Final device and emulator runtime proofs use the matched products. A
  source-only consumer does not replace those proofs.
- Private proof inputs can use `XMTP_SDK_GENERATED_DIR` and
  `XMTP_SDK_PACKAGES_DIR`. Prebuilt runtime inputs can use
  `XMTP_SDK_RUNTIME_DIR` (a directory with core/node or core/wasm products).
  These variables configure build tools. They add no SDK runtime option.

Package preparation does not publish a product.

Package review checks:

- `dev/nix-shell 'just sdk check-package-scripts'` also checks both provenance producers,
  config-only changes, Cargo compiler overrides, macOS deployment targets,
  Windows browser asset paths, the mobile staging checks, cache inputs, and
  both flat and prebuilt runtime directory layouts.
- The private compiler input is `XMTP_SDK_TSDOWN_CLI`. It names tsdown's
  `dist/run.mjs`, which the stager runs through Node on every platform.
  Windows CI stages with a supported compiler Node version and runs the
  installed smoke on the minimum SDK Node version, 22.12.0.

Residual package review checks:

- Build provenance includes the live address registry, chain URL map, and
  signature validation bytecode. The common receipt producer uses the same
  fingerprint. Mobile staging requires each Android receipt's exact triple.
- Installed smoke runs `npm-cli.js` through Node. `XMTP_SDK_NPM_CLI` is a
  private path override for the launcher proof. The normal path comes from
  the selected Node installation, including the Windows installation.

- Cargo uses its normal or caller-selected job count. Native stages use
  vendored static OpenSSL. Apple stages keep macOS 11 and iOS 14 floors; the
  Nix evaluation asserts these inputs. They do not prove archive linkage or
  installed package loading.
Android staging dependency inputs:

- The staging Gradle root is `sdks/android`. Its `library/gradle.lockfile`,
  `buildscript-gradle.lockfile`, and `gradle/verification-metadata.xml` must
  exist before the stage runs Gradle.
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
- `sdks/android` owns its own Gradle graph. Do not copy staging lock state
  into a different Gradle root.

- The Android stage builds `sdks/android/:library:assembleRelease`.
  It checks that selected SDK root's `buildscript-gradle.lockfile`,
  `library/gradle.lockfile`, and `gradle/verification-metadata.xml`.
  The staging fixture inputs do not cover this graph. An explicit
  `--sdk-root` selects the root whose inputs and output are used.

Migration tests use the normal SDK packages. Node and Agent use their existing
Vitest suites. Browser fixtures and worker fault checks are in
`sdks/browser/test/platform/migration`, included by `just sdk test-browser`.
Run only those checks with `dev/nix-shell 'just sdk test-browser migration'`
after `generate`. The platform runner stages the matched Browser package. They check import, worker ownership,
failed output, page termination, and source preservation. The small fixture
worker uses raw legacy bytes and is not part of a public package.
Swift uses `LegacyMigrationTests` in the normal XCTest target. Kotlin uses
`LegacyMigrationTest` in the normal JUnit target. Run
`dev/nix-shell 'just ios test all --filter XmtpSdkTests.LegacyMigrationTests'` or
`dev/nix-shell 'just android test --tests uniffi.xmtp_sdk.LegacyMigrationTest'`. The iOS unit job and
the Android `sdk-build` job include these tests.
