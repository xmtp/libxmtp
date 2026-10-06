# SDK cutover handoff

Status: **PREPARATION ONLY. Phase 2 is not admitted.** The migration lane starts
at `5e33c2faac0f91b5c55ebf42cba06836c9c47c00`, which contains A #4334 and B #4339.
That commit does not contain the full merged Phase 1 stack. Earlier tests and
staged packages do not establish final gates. The current
[execution plan](https://plan.ref.tools/TiFDtuzx3U19olnv) owns P33–P37.

## One merged baseline

Root is the single integration writer and owns the following receipt. Before
Phase 2 implementation, fill every pending field with evidence from the actual
merged head. A local stack or an unmerged lane cannot fill it.

| Receipt | Value |
| --- | --- |
| Full Phase 1 PRs and required forward fixes, each actual mergedAt | PENDING |
| Containing `self-hosted` commit | PENDING |
| API surface and façade source hash | PENDING |
| Generator source hash and matched build inputs | PENDING |
| Swift XCFramework/package hash | PENDING |
| Kotlin AAR/JNI/package hash | PENDING |
| Node package and each supported native binary hash | PENDING |
| Browser package, worker WASM and pure WASM hashes | PENDING |
| V14 release/callback gate on those packages | PENDING |
| V17 merged baseline, ownership and independent track gate | PENDING |
| All switched package versions | 8.0.0; set during Phase 2; publication is separate |

Starting pins: UniFFI `0.32.2`, generator backend fork
`330f9edbc3c4d6e6e948f6d2eb2724358668b79a`, and Cargo workspace version
`1.12.0-dev`. Check `Cargo.toml`, `Cargo.lock`, `nix/lib/packages/ubrn.nix`, and
the final package build manifest together at the merged baseline. Record any
changed pin. `dev/release-tools/src/lib/sdk-config.ts` currently has only the
old package version tracks. Its common update belongs to Root inside the
owning SDK PR. Do not change versions or run a release workflow in Phase 1.

## File ownership and consumer audit

The temporary cutover audit and its path ledger are retired. The owner map
below records the original lane scope. Current public consumer fixtures and
target tests remain in the repository.

| Writer | Scope and exact assignment rule |
| --- | --- |
| iOS, PR F | `sdks/ios/**`, `Package.swift`, Apple `Package.resolved`, `nix/package/ios.nix`, `crates/xmtp_sdk/conformance/swift/**`; workflows `lint-ios.yml`, `release-ios.yml`, `test-ios.yml` |
| Android, PR G | `sdks/android/**`, `apps/android/xmtpv3_example/**`, `nix/package/android.nix`, `crates/xmtp_sdk/conformance/kotlin/**`; workflows `lint-android.yml`, `release-android.yml`, `test-android.yml` |
| Node plus agent, PR H | `sdks/node/**`, `sdks/agent/**`, `apps/cli/**`, `nix/package/node.nix`, `crates/xmtp_sdk/conformance/ts/**`; workflows `lint-node.yml`, `release-agent-sdk.yml`, `release-cli.yml`, `release-node-sdk.yml`, `test-agent-sdk.yml`, `test-node-sdk.yml`; docs examples ending `-node.ts` and starting `agents-` |
| Browser, PR I | `sdks/browser/**`, `apps/web-chat/**`, `nix/shells/wasm.nix`, `nix/package/wasm-nextest.nix`, `crates/xmtp_sdk/conformance/browser/**`; workflows `deploy-web-chat.yml`, `release-browser-sdk.yml`, `test-browser-sdk.yml`; docs examples ending `-browser.ts` |
| Root, integration writer | All remaining audited paths, including every shared `.mdx`, common example, legacy binding, public consumer fixture, migration fixture, mixed workflow, lockfile, workspace, Just recipe, release helper, waiver, manifest and test-map edit |

Every shared `.mdx` has one owner: Root. Platform examples are separate source
files. Common content-type examples also belong to Root. No platform writer
edits a common runtime, generator, public API, or artifact contract. Send those
changes to Root. Root applies shared-file changes serially inside the SDK PR
that needs them. `apps/xmtp_debug/**` consumes Rust `xmtp_mls` directly; Root
owns its Cargo changes. A sibling SDK switch is not a platform prerequisite.

At the start checkpoint, the manifest has 3389 rows and the mobile test map has
176 rows. The manifest lists 136 open entries. The owner's recorded item 8
already approves all 47 rows still labelled `proposed removal`: Swift
ContentCodec equality/hash/id/description, the native legacy ReactionCodec
families, and the fifteen #4327 binding-root names. Preserve that approval.
The common manifest rules need to record it. The other entries need current
replacement evidence or a separate decision. An inventory status alone does
not prove that a final public member exists. Root records their final
classification and public-root proof in the owning PR.

## Guide and target proof

These checks are independent. Record the exact command, commit, package hashes,
compiler/runtime result and raw output for every row. Unrun means PENDING.

| Host | Compiler and guide runtime | Target/consumer switch proof | Status |
| --- | --- | --- | --- |
| Swift | Compile `Migration.swift` in the installed XmtpSdk consumer; call `exerciseMigration` with the package signer and persistent paths in an app bundle | Swift public consumer, iOS package/device/lifecycle examples; V14 target callback/release gate | PENDING |
| Kotlin | Compile `Migration.kt` in the installed Android consumer; call `exerciseMigration` on JVM and emulator with Android paths | Context default storage, AAR/JNI packaging, emulator/lifecycle examples; V14 target gate | PENDING |
| Node plus agent | Compile `node.ts` through the installed ESM root; call `exerciseMigration` with real signer and existing database | Native supported platforms, Node/agent/CLI tests and distinct docs examples; V14 target gate | PENDING |
| Browser | Compile `browser.ts` through the installed ESM root; call it in Chromium with OPFS entries | Worker/pure assets, OPFS/import/export/locking, web consumers; V14 target gate | PENDING |

Focused review repair proof uses immutable generated inputs at
`a8c08c2e81f719284cb1f491adf0576f7210aed7` and local installed Node/browser
tarballs. Swift, Kotlin and Node pass absolute and
`./existing.db3` opens of the same file. The old verbatim comparisons fail on
the relative form. Android context storage passes create and offline build on
the JVM through the actual helper with a controlled `Context.filesDir` value.
This does not close the Android device/emulator row. Chromium pure codecs fail
when initialization is omitted and pass after `await initPureWasm()`. These
focused products record `final_gate = false`; every target row above stays PENDING.

After public staging, compile each guide example in this same worktree:

```sh
dev/nix-shell 'bash docs/self-hosted/sdk-migration/check-examples swift'
dev/nix-shell 'bash docs/self-hosted/sdk-migration/check-examples kotlin'
dev/nix-shell 'bash docs/self-hosted/sdk-migration/check-examples node'
dev/nix-shell 'bash docs/self-hosted/sdk-migration/check-examples browser'
```

The compiler helper never overwrites an existing fixture. It removes only the
copy it makes. Runtime calls still need a real signer/backend, persistent paths,
and the named host environment. Record them separately.

Existing commands from the repository root:

```sh
dev/nix-shell 'just sdk generate'
dev/nix-shell 'just sdk public-consumer'
dev/nix-shell 'just sdk conformance swift'
dev/nix-shell 'just sdk conformance kotlin'
dev/nix-shell 'just sdk conformance browser'
dev/nix-shell 'just spec-check'
```

Use the worktree backend from `dev/nix-shell 'just backend status'`. Start only
services needed by the named proof. Request the shared costly-build slot before
native or WASM generation. The package lane supplies the final package-smoke
command and assets; do not claim a planned recipe has run. Add its exact command
and result to the merged receipt. The existing supported reader forms are documented in the guide. Swift loop
exit triggers teardown without awaiting it; Kotlin Flow finalization awaits
end; Node/browser async iteration calls `return()` on early exit. The frozen reader lane
`cd6d1332e4712699a6e13b829ed78937c53b7877` passes all 16 host exit modes with
real Rust reader leases and fails all 16 disabled-cleanup controls. Node and
Chromium use public factories and the real worker. Native probes retain raw
handles. Completion uses synthetic EOF at the host seam with a real lease.
Final integrated installed-package, platform, and lifetime checks remain PENDING. The private logging admission hook
is an internal safety detail and adds no public app API.

The owner excludes one callback pattern: do not await `Client.end()` on the
owning client inside `CredentialSource.credential`. Return from the callback
first, then end that client. External end while the callback is held and
independent-client reentry remain required lifetime checks. A callback that
never returns has no thread-release guarantee.

## Cutover checks

- Every manifest row has an implemented public member or recorded owner-approved
  removal. Public-root consumers require no `.raw`, deep import, alias, CommonJS
  fallback, or legacy codec adapter.
- All target package, conformance, guide compiler/runtime, current DB compatibility,
  and callback/lifetime checks pass at the matched integrated candidate.
  Required core fixes are merged.
- The paired old-vs-new benchmark gate (the 20% regression rule) is retired
  with the cutover. The old SDKs are not in the repository, so no paired run
  is possible. A single-side suite records absolute numbers for trend tracking.
  It has no pass or fail line.
- Close a waiver only with proof for its entire row on every applicable host at
  the same candidate. Mixed core/SDK rows need both parts. Add the new `verifies:`
  links and remove the fully proved waiver in the same owning PR. Keep valid
  `untested` rows and partial host gaps. A core backlink does not close a host gap.
  Preserve #4342 archive cuts and accepted gaps; do not restore legacy V1 retention.
- Branch F–I independently from the recorded merged Phase 1 commit. First prove
  each switch with all other SDKs still old. On its current base, run its target
  gate again on the assembled package, plus affected mixed-state workspace, docs,
  consumer, workflow and conformance checks. After siblings merge or common files
  change, rebase and repeat affected checks on the final PR head.
- H removes `bindings/node` when it moves its last consumer. I removes
  `bindings/wasm` when it moves its last consumer. The second of F/G removes
  `bindings/mobile` and `apps/android/xmtpv3_example`. Root performs shared
  deletion edits inside that same SDK PR. Keep only bindings with a named
  remaining repository consumer.
- The PR that removes the last old SDK also deletes `dev/sdk/inventory.py`,
  `dev/sdk/manifest_rules.py`, `dev/sdk/binding-test-map.tsv`,
  `docs/self-hosted/sdk-api-manifest.md`, their Just recipes, CI references, and
  this temporary audit generator/receipts. Keep the migration prose and small
  public consumer fixtures. Do not create a new member database. Search for stale
  references and run surviving checks before merge.
- J starts after F–I merge. It removes only remaining shared cleanup and proves
  no orphaned binding, callback, or dependency remains. The PR removing the last
  app-data callback consumer replaces its callback proof with an event-plus-read
  proof that fails when the event is suppressed. React Native starts after J.
  Publication requires a separate owner action.
