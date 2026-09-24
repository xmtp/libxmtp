# xmtp-js Teardown (Phase C) — run AFTER browser/node-sdk first publish from libxmtp

Prerequisite: `@xmtp/browser-sdk` and `@xmtp/node-sdk` have been published from
libxmtp (release-browser-sdk.yml / release-node-sdk.yml) so the npm versions exist.

## Known follow-up: one browser-sdk test asserts against an older binding

`sdks/js/browser-sdk/test/Group.test.ts > "should filter messages with options"`
asserts `messages.length === 13`, but against the locally-built binding
(`1.11.0-dev`, current libxmtp HEAD) it returns 14. The test was written against
the published nightly binding (`1.11.0-nightly.20260603`); co-locating now runs it
against live binding source, surfacing a message-filtering behavior drift between
those versions. This is NOT a migration defect (195/208 browser-sdk tests pass on
shard 1/4; node-sdk is 179/179). Resolve as a behavioral decision by the SDK/binding
owner: either update the assertion to match current binding behavior, or investigate
whether the binding changed message filtering intentionally. Left unmodified so the
drift is visible rather than masked.

In the xmtp/xmtp-js repo:
1. Remove `sdks/browser-sdk` and `sdks/node-sdk`.
2. Remove their CI: `.github/workflows/browser-sdk.yml`, `node-sdk.yml`.
3. In `.github/workflows/release.yml`, drop the prerelease / auto-prerelease-bindings
   handling for browser-sdk + node-sdk (that automation now lives in libxmtp).
   Keep agent-sdk handling.
4. Rewire consumers to published npm versions:
   - `apps/xmtp.chat`: `@xmtp/browser-sdk: workspace:^` -> `^7.x` (published).
   - `sdks/agent-sdk`: `@xmtp/node-sdk: 6.0.0` (workspace-resolved) -> `^6.x` (published).
   - `packages/xmtp-cli`: `@xmtp/node-sdk: 6.0.0` -> `^6.x` (published).
   - content-types devDeps already use published `@xmtp/node-sdk@4.6.0`; bump as needed.
5. Update `renovate.json` to also track `@xmtp/node-sdk` / `@xmtp/browser-sdk`.
6. Remove the now-orphaned changeset config entries for the two SDKs.
7. `yarn install` to regenerate the lockfile; build agent-sdk / xmtp-cli / xmtp.chat
   to confirm they resolve the published SDKs.
8. Cross-repo invariant: `@xmtp/content-type-primitives` and the (now-external)
   `@xmtp/node-sdk` must reference the same `@xmtp/node-bindings` version; Renovate
   keeps both on the same published nightly.

## libxmtp-side prerequisite (already done in the migration PR)
The two SDKs are registered in `dev/release-tools/src/{types.ts,lib/sdk-config.ts}`
with `versionTrack: "independent"`, and `.github/workflows/release-{browser,node}-sdk.yml`
build the SDK bundle against the nix-built bindings and publish via npm-publish.yml.

### Phase B activation — remaining before the FIRST publish (libxmtp repo)

Two deliberate steps are NOT done yet (they are release-policy decisions, kept
out of the migration PR):

1. **Hub wiring.** `.github/workflows/release.yml` uses explicit per-SDK jobs
   (`release-node`, `release-wasm`) + `workflow_dispatch` boolean inputs +
   nightly `schedule` gate; it does NOT auto-fan-out via the registry
   `releaseWorkflow` field. To publish browser/node-sdk, add `browser-sdk` /
   `node-sdk` dispatch inputs, mirror them in the `validate` resolve step, and
   add two fan-out jobs calling `release-browser-sdk.yml` / `release-node-sdk.yml`
   (model on `release-node`/`release-wasm`). Decide whether they join the nightly
   `schedule` or stay manual-dispatch-only initially.

2. **portal: -> published binding version at publish time.** The SDK
   `package.json` lists its binding as `@xmtp/<x>-bindings: "portal:..."`, which
   is NOT publishable to npm. Before `npm publish`, the binding dep must be
   rewritten to the published binding version (the version produced by the
   libxmtp `release-node`/`release-wasm` run for the same ref). Add a step in
   `release-{node,browser}-sdk.yml` (or extend `xmtp-release set-manifest-version`)
   to set `dependencies["@xmtp/<x>-bindings"]` to that published version before
   the publish job runs. Without this, the published package would carry a
   broken `portal:` spec.
