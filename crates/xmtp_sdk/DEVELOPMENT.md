# SDK generation and package checks

Run commands from the repository root through `dev/nix-shell`.
The [generator guide](../../apps/xmtp_sdk_bindgen/README.md) owns metadata markers
and hand-maintained projections; [generated lint](../../apps/xmtp_sdk_bindgen/GENERATED_LINT.md)
explains runtime copies and typechecking.

## Generation routes

`just sdk generate [swift,kotlin,node,browser]` builds then renders selected
artifacts into `target/sdk-generated/`. RN uses `just sdk generate react-native`.
`XMTP_SDK_BUILD=cargo` is the default (debug unless `--profile release`);
`nix` copies release-profile selected products. Prefer `nix` for work that edits
no SDK crate; CI JS and docs use it.

The Cargo route normally uses Nix bindgen, but builds a debug generator when its
inputs differ from the merge base with `origin/main`. CI keeps Cargo bindgen;
`XMTP_SDK_BINDGEN=nix|cargo` overrides selection. Generation needs no pnpm install.
Both routes render with the pinned Oxfmt and write receipts.

`just sdk build [targets]` records artifact hashes in `target/sdk-artifacts`.
`just sdk render [targets]` checks them, replaces only selected trees, and rejects
stale Node/Browser inputs. Mobile preflight checks Swift/Kotlin later. Rebuild
before rendering after Rust/generator edits. Stage new files before recording
provenance: Nix evaluates Git-tracked inputs.

`index.ts` is the public projection; stock UniFFI `binding.ts` stays private.
Node/RN public layers import it for native loading. Each TS tree links its pinned
runtime packages. RN uses its own fork runtime and prebuilt native libraries.

## Package preparation

`just sdk stage node|browser` compiles ESM products and copies complete native,
worker, pure WASM, loader, snippet, and runtime assets. Staging requires exact
asset sets and hashes; extra files fail. `dev/js/sdk-package` copies public
products to SDK `dist` for workspace imports; release packs `target/sdk-packages`.
`just sdk package-smoke node|browser` installs a packed product in an empty
consumer; Browser also loads its worker. Preparation never publishes.

Nix is the native Android/iOS producer. `just sdk mobile-stage ios` copies
release XCFrameworks and checks slices against `nix/sdk-platforms.json`.
`sdks/ios/dev/bindings --release --build-only` selects canonical or local output;
local fallback is an error in CI. For Android, generate Kotlin with
`XMTP_SDK_BUILD=nix`, then `just sdk mobile-stage android` assembles the AAR over
all ABI libraries and checks each receipt against Kotlin. Keep Kotlin's
`-Xjvm-default=all`.

Private build inputs: `XMTP_SDK_GENERATED_DIR`, `XMTP_SDK_PACKAGES_DIR`, and
`XMTP_SDK_RUNTIME_DIR` (core/node or core/wasm). These do not add public options.
`XMTP_SDK_TSDOWN_CLI` names `dist/run.mjs`, always run through Node. Windows
staging uses compiler-supported Node; installed smoke uses the SDK minimum.
`XMTP_SDK_NPM_CLI` overrides only the smoke launcher's npm path.

Keep vendored static OpenSSL and Apple deployment floors in both dependency and
final builds; `base.assertNativeEnv` checks these at evaluation. Receipts include
SDK source/render identities, embedded registry/chain/bytecode inputs, and exact
Android triples. Configuration-only edits must invalidate the appropriate product.

## Android dependency refresh

The staging root is `sdks/android`. Its `buildscript-gradle.lockfile`,
`library/gradle.lockfile`, and `gradle/verification-metadata.xml` must exist;
normal staging uses strict verification and never writes them.

Refresh separately in the Android Nix shell against the actual `assembleRelease`
route with `--write-locks --write-verification-metadata sha256`. Review the plugin
classpath, release configurations, repositories, and hashes; output hashes alone
prove no input coverage. Keep metadata verification on and each `<component>`
on one line without dropping checksum/policy entries. Do not copy locks into a
different Gradle root. An explicit `--sdk-root` selects both inputs and output.

## Selecting checks

| Change | Recipe (through Nix) |
| --- | --- |
| Packaging scripts, receipts, cleanup | `just sdk check-package-scripts` |
| Nix generated/mobile closures | `just sdk check-generated-nix` (evaluation only) |
| Source conventions | `just sdk lint-source` |
| Generated surface/runtime | generate, then `just sdk lint-generated` and `just sdk test-bridge` |
| WASM loading | `just sdk wasm-init` |
| Browser platform behavior | `just sdk test-browser` |
| Focused browser proof | `just sdk test-browser-logging`, `test-browser-storage`, or `test-browser-package` |
| Browser migration | `just sdk test-browser migration` |
| Swift lifecycle | `just backend ci just ios test-lifecycle` |
| Swift/Kotlin negative consumers | `just ios check-consumer` / `just android check-consumers` |
| Wallet secret copies | `just test workspace -p xmtp_sdk --features xmtp_cryptography/test-utils --test zeroize` |

Generate matching targets before runtime checks. Test hooks stay in test sources;
public Swift/Kotlin/TS outputs and runtime sources must omit them. Packaging
Python/Node tests are discovered automatically; fixtures do not prove installed
native execution. Use release/device routes before merge: release libraries
abort on panic while debug libraries unwind.

Browser platform tests copy Nix-built codec/panic/legacy fixtures and use an
owned loopback object-store relay (`sdks/browser/test/platform/object-store`).
It rewrites only the upload URL and preserves gRPC status trailers. Node's
held-upload test owns a separate relay; it needs neither that fixture nor protoc.
Migration uses normal public packages; Rust converter tests cover Swift/Kotlin.

Façade tests need the backend. Custom handle settings use `TestServer` with its
own database on this worktree's PostgreSQL. Heap-canary tests need no backend.

RN native production and installed consumers:
[RN package development](../../sdks/react-native/DEVELOPMENT.md).
Benchmarks and remote-backend opt-in: [benchmarks](benchmarks/README.md);
`just sdk bench-check` needs no SDK build, backend, or device.
