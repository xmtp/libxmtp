# XMTP SDK façade

Run commands from the repository root in the Nix shell. Run
`dev/nix-shell 'just backend status'` to find this worktree's backend ports.

- `dev/nix-shell 'just sdk generate'` builds the SDK libraries and writes Swift, Kotlin, Node,
  worker WASM, and pure browser WASM bindings to `target/sdk-generated/`. In
  each TypeScript tree, `index.ts` is the package root: the public layer that
  the projection generates. The stock UniFFI root is the private `binding.ts`.
  The Node public layer imports it to load the native binding; otherwise only
  the worker, the benchmark, and transport tests import it.
- `dev/nix-shell 'just sdk check-native-nix'` evaluates native build inputs and compares
  the checkout source identity with the generated and native Nix source filters.
  It does not compile a product.
- `dev/nix-shell 'just sdk check-file-sizes'` checks the 1,000-line limit for every SDK source
  file, including conformance files. Generated and ignored build files are excluded.
  Keep most new files below 500 lines.
- `dev/nix-shell 'just sdk lint'` checks file sizes, generated names, and TypeScript source.
  It checks shared public value types on Node and browser, including negative
  consumers for readonly records, transport fields, credentials, and bytes. It also
  rejects test-only hooks (`*ForTest`, `*_for_test`, `bridge_test_panic`) and
  benchmark exports in the default bindings and in
  `apps/xmtp_sdk_bindgen/runtime/`. Keep test hooks in test source sets.
- `dev/nix-shell 'just sdk wasm-init'` loads the staged WASM package in Node.
- `dev/nix-shell 'just sdk conformance <swift|kotlin|node>'` runs scenarios against this
  worktree's backend. `dev/nix-shell 'just sdk conformance browser'` runs scenarios 1-11
  plus a real WASM trap from a test-only panic fixture in Vitest Playwright
  Chromium, then checks real OPFS and worker behavior. Its recipe builds the
  pure codec and panic fixtures in the Rust shell before the JS shell.
  Scenario 7 checks readers and streams. Scenario 8 checks events and listeners.
  The browser run also checks storage layouts and attachments, with failure
  records in the conformance-featured panic fixture. Worker death uses the
  generated public package and its shared worker manager.
  All host runs start `conformance/ts/object-store.mjs` for their
  download fixtures. The default ephemeral fixture port keeps `SDK_FIXTURE_URL`
  separate from native S3 on port 9067. Swift CI uses
  `dev/nix-shell 'just sdk generate swift'`, then
  `dev/nix-shell 'just backend ci just sdk conformance swift'`. Each run sets `SDK_RELAY_TARGET` to the backend. The fixture can hold a
  small PUT response, count upload grants and object requests, and refuse
  selected relayed backend URLs. It uses `protoc` from the Rust shell to
  replace only the upload URL in a real backend response. Native clients use
  the fixture's HTTP/2 relay; browser clients use its gRPC-web relay. Both
  preserve gRPC status trailers.
- `dev/nix-shell 'just sdk cutover-bench <swift|kotlin|node|browser> <config> <output>'` runs
  the installed release benchmark. See `benchmarks/README.md` for the package
  closure, adapters, metadata, and required checks. It records 20 or more
  pairs. The release gate stays pending until package and callback review.
- `dev/nix-shell 'just sdk cutover-bench-ios-prepare <config> <output>'` prepares a Release
  UIKit app for the installed old or new public Swift product. Then run
  `NIX_DEVSHELL=ios dev/nix-shell 'just sdk cutover-bench-ios-build <output> <simulator-udid> <derived-data>'`.
  Use separate side directories.
  `dev/nix-shell 'just sdk cutover-bench-ios-controls <host-config> <output>'` checks real app
  memory, signer HTTP, identity rejection, timing scope, and timeout cleanup.
  See `benchmarks/README.md` for the HTTP signer and app launch configuration.
- `dev/nix-shell 'just sdk cutover-bench-check'` checks the statistical gates and receipt rules.
  It also checks new and old Node/browser package admission and deterministic
  browser long-task interval boundaries and awaited stream cleanup outside timing.
- `dev/nix-shell 'just sdk cutover-bench-swift-delay-check <output>'` compiles
  the exact Swift delay helper and checks zero, ordinary, maximum and overflow inputs.
- `dev/nix-shell 'just sdk cutover-bench-stream-check <output> <browser-node_modules>'` checks
  live stream content in Node, Chromium, and compiled Swift/Kotlin helpers.
  It requires missing or changed live content to fail with correct history.
- `dev/nix-shell 'just sdk cutover-bench-baselines <output>'` resolves published baseline
  versions and records source and artifact hashes.
- `dev/nix-shell 'just sdk bench'` is an internal diagnostic. It compares 20 release-profile Node calls for a zero-row page
  and a 10,000-message page with the current Node binding. It also measures
  one empty SDK async call. It runs Node with `NODE_ENV=production`. It
  enables the off-by-default `bench` feature and writes separate bindings to
  `target/sdk-bench/`.
- `dev/nix-shell 'just sdk caller-cancellation-swift'` checks cancelled nonthrowing calls and
  real reader pre-poll, pending and READY handoff. It counts native cancel/free
  calls in generated conformance copies and requires the prior item to replay.
  It also checks EventReader and event iterator cancellation before polling,
  while pending, and after READY or lift. Ended event reads return nil.
  Constructor lifetime checks retain the READY gate and add a post-lift gate.
  The post-lift control requires no native cancel, one complete and free,
  weak owner release, stopped workers, and a disconnected store before cleanup.
- `dev/nix-shell 'just sdk callback-lifetime <swift|kotlin|node>'` runs 20 held callback
  and constructor adoption cycles against fresh conformance bindings. Set
  `SDK_CALLBACK_LIFETIME_FAMILY` to select one family. These bindings expose
  real foreign task and callback handle counts only for conformance.
  `dev/nix-shell 'just sdk callback-lifetime browser-transport'` runs 20 real-worker cycles
  for completion, session close, and worker death. It proves transport behavior;
  it does not replace a generated browser SDK proof. Install JS dependencies
  first with `dev/nix-shell 'just install-js'`.
- `dev/nix-shell 'just sdk check-conformance-targets'` checks Swift, Kotlin, Node, browser, mixed,
  and default target selection with the real counter injector. It uses generated
  bindings and a renderer fixture. It does not build Rust libraries.
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
- `dev/nix-shell 'just sdk public-consumer'` stages the generated Swift, Kotlin, Node, and
  browser SDKs as separate public products under `target/sdk-public/`. It then
  compiles separate consumers in `conformance/public/`: a SwiftPM package, an
  Android library that uses a real `Context`, and TypeScript projects that
  install the Node and browser packages in `node_modules`. The consumers call
  the retained public Client surface, the identity unions, received identity,
  and Message actions on the package roots. Each Swift/Kotlin codec record has
  isolated wrong-value probes for encode, send, and reply. Negative probes check that the
  binding Client, its factories, the generated identity routes, the browser
  worker session, and private package paths stay private. The Node root must
  export exactly the public names through ESM imports. It has no CJS or
  `require` entry point. Its engine floor is Node 22.12. Before it compiles them,
  `dev/check-public-members.py` checks that every retained Client member in
  `docs/self-hosted/sdk-api-manifest.md` is public in each installed product,
  in the static or instance placement that the manifest names.
  Run `dev/nix-shell 'just sdk generate'` first.
- `dev/nix-shell 'just sdk codec-author types'` stages independent Node and browser codec
  packages and checks valid calls plus six wrong-value rejections per target.
  `dev/nix-shell 'just sdk codec-author node'` and `dev/nix-shell 'just sdk codec-author browser'` also run
  encode/send/receive/reply checks against this worktree's backend; the browser
  run uses Chromium and the real package worker. Both read this worktree's
  Docker backend to check published push flags. Run SDK generation first.
  `XMTP_SDK_GENERATED_DIR` can select a separate generated input directory.
  For final package checks, `XMTP_SDK_PACKAGES_DIR` selects staged `node` and
  `browser` package folders and preserves their manifests, bundled dependencies,
  and assets.
  The proof installs local copies under `target/sdk-codec-author/` and uses
  only the supported ESM roots in the codec package.
- `dev/nix-shell 'just sdk manifest-check'` and
  `dev/nix-shell 'just sdk-manifest-check'` check the same API manifest.
  A switched SDK keeps its pinned pre-switch retention and removal ledger.
  The same manifest counts its current public projection in a separate section.
  Unswitched SDKs still match their current source declarations. Missing current
  generated source or changed pinned rows fail. The checks also test rejection
  of missing projections, altered ledger rows, and unswitched sibling changes.
  Run generation first for each switched target. `XMTP_SDK_GENERATED_DIR`
  selects the matched generated source input. After a source change, update the
  current count with `dev/nix-shell 'python3.11 dev/sdk/inventory.py --write'`.
  The SDK check also verifies retained TypeScript root exports for switched
  targets. Before any switch, it verifies both TypeScript roots.
- `dev/nix-shell 'just test crate xmtp_sdk'` runs the façade tests against the local backend.

The generator lives in `apps/xmtp_sdk_bindgen/`. Its global UniFFI config maps
`xmtp_sdk` to this crate root so Swift and Kotlin load `uniffi.toml`.

Content and conversation exports use named `include!` files to keep UniFFI
module paths stable. Use ordinary modules for helpers without exported metadata.

## Matched package preparation

- `dev/nix-shell 'just sdk build [swift,kotlin,node,browser]'` builds selected artifacts once.
  It records file hashes under `target/sdk-artifacts/`. Native targets do not
  build WASM. Full and pure WASM use separate output directories.
- `dev/nix-shell 'just sdk render [swift,kotlin,node,browser]'` uses those artifacts. It rejects
  a changed binary or generator contract before it replaces generated output.
  Package staging requires exact generated asset sets and hashes. Unlisted
  generated files fail before runtime or compiler work.
  It replaces only selected targets and keeps valid unselected targets with
  their original receipts. It removes stale unselected targets and unknown roots.
  `dev/nix-shell 'just sdk generate [targets]'` runs both steps. Use `--profile release` on the
  build recipe for release proofs. Conformance shares the bindgen artifact.
- `dev/nix-shell 'just sdk check-package-scripts'` checks reuse, mismatch rejection, and cleanup
  with a small fixture. `dev/nix-shell 'just sdk check-clean-generate'` adds one real Swift
  render. It uses existing artifacts and does not rebuild Rust.
  The script checks also reject missing or extra pure WASM functions and pure
  functions in worker bindings or dispatch. The exact set comes from approved
  pure Rust declarations in the current isolated SDK source.
- `dev/nix-shell 'just sdk stage node'` and `dev/nix-shell 'just sdk stage browser'` compile ESM products with
  tsdown. They copy the pinned runtimes, native library, worker, pure WASM,
  loaders, and snippets. `dev/nix-shell 'just sdk package-smoke node|browser'` packs each product
  and installs it in an empty consumer. It checks a codec round trip and rejects
  a changed contract before an operation. Browser smoke also loads its worker.
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
  Explicit target OpenSSL roots keep upstream library-directory selection. Host
  library and header paths use the selected compiler's host-qualified variables
  in the Android child environment. Parent inputs and caller overrides stay intact.
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
