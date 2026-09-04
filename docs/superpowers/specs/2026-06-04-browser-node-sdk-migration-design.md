# Browser SDK + Node SDK Migration into libxmtp — Design

**Date:** 2026-06-04
**Status:** Approved (design phase)

## Goal

Move two SDKs — `@xmtp/browser-sdk` and `@xmtp/node-sdk` — out of the
`xmtp/xmtp-js` monorepo and into this `libxmtp` monorepo, alongside the Rust
bindings (`bindings/wasm`, `bindings/node`) they consume. After migration:

- The SDKs source the libxmtp-built bindings locally (via Yarn Berry `portal:`),
  not published npm pins.
- Their git history (commits touching only those two folders) is preserved.
- `just` recipes + CI workflows mirror the established android/ios precedent.
- CI reuses the nix-cached binding builds (no Rust rebuild) and gates SDK tests
  on the binding tests succeeding.
- The remaining xmtp-js packages that depended on the two SDKs are rewired to
  published npm versions, sequenced after the SDKs first publish from libxmtp.

## Non-goals

- Migrating other xmtp-js packages (content-types, agent-sdk, xmtp-cli,
  xmtp.chat). They stay in xmtp-js.
- Switching the SDKs off Yarn Berry / `node-modules` linker.
- Building the SDK `dist` as nix derivations (low payoff — see Section 3).

---

## Section 1 — Repository structure & history import

### Target layout

```
sdks/js/                          ← new Yarn Berry workspace root
  package.json                    (private; workspaces: ["browser-sdk","node-sdk"])
  yarn.lock  .yarnrc.yml  .yarn/releases/yarn-4.10.3.cjs
  js.just                         ← wired into root justfile via `mod js`
  dev/.setup  dev/bindings        ← nix-build bindings → populate bindings/<x>/dist
  browser-sdk/                    (@xmtp/browser-sdk, imported with history)
  node-sdk/                       (@xmtp/node-sdk, imported with history)
```

Bindings remain at `bindings/node` + `bindings/wasm` (siblings of `sdks/`, NOT
workspace members). SDKs reference them via `portal:` (Section 2).

### History import (jj-safe)

The working dir is a **secondary jj workspace**; the colocated git store lives at
the `main/` workspace. Run jj operations from `main/`. The two folders have
always lived at `sdks/browser-sdk` / `sdks/node-sdk` in xmtp-js, so a
`--path-rename` to `sdks/js/` is needed for the destination.

```bash
# Extract both folders with history into a scratch git repo
git clone https://github.com/xmtp/xmtp-js.git /tmp/xmtp-js-extract
cd /tmp/xmtp-js-extract
git-filter-repo \
  --path sdks/browser-sdk/ --path-rename sdks/browser-sdk/:sdks/js/browser-sdk/ \
  --path sdks/node-sdk/    --path-rename sdks/node-sdk/:sdks/js/node-sdk/
# → ~415 commits; authorship + dates preserved; 186 shared commits deduped;
#   all unrelated paths (bench/node-sdk, examples/*, workflows) stripped.

# Land into the jj monorepo from the colocated main workspace
cd <libxmtp>/main
jj st                                      # ensure clean
jj git remote add xmtpjs /tmp/xmtp-js-extract
jj git fetch --remote xmtpjs --branch main # restrict to the one filtered branch
jj new main main@xmtpjs -m "Import xmtp-js browser-sdk + node-sdk history into sdks/js"
jj st                                      # verify sdks/js/{browser,node}-sdk present
jj bookmark move main --to @               # jj bookmarks don't auto-advance
jj git remote remove xmtpjs
```

`git-filter-repo` is not installed; fetch the standalone script to `/tmp` or add
it to the devShell. It keeps a commit if it touched any `--path` and strips other
paths from that commit; commits that become empty are pruned. Author/committer
identity and both timestamps are preserved; only SHAs change.

The `sdks/js/` workspace-root scaffolding (root `package.json`, `js.just`,
`dev/`, `.yarnrc.yml`, `.yarn/`) is added as a **follow-up commit on top** of the
imported history.

---

## Section 2 — Workspace, binding injection, just recipes

### Workspace root (`sdks/js/package.json`)

```json
{
  "name": "@xmtp/js-sdks",
  "private": true,
  "packageManager": "yarn@4.10.3",
  "workspaces": ["browser-sdk", "node-sdk"]
}
```

`.yarnrc.yml`: `nodeLinker: node-modules` (required — `portal:` + the nix `cp`
inject need a real `node_modules` tree; PnP is not supported by the inject
mechanism). Carry over `.yarn/releases/yarn-4.10.3.cjs` from xmtp-js.

### Binding references (the only content edit to imported SDKs)

Yarn Berry workspace globs cannot escape the workspace root with `../`, so the
bindings can't be workspace members of a `sdks/js/` root. Use `portal:`:

- `sdks/js/browser-sdk/package.json`: `"@xmtp/wasm-bindings": "portal:../../../bindings/wasm"`
- `sdks/js/node-sdk/package.json`: `"@xmtp/node-bindings": "portal:../../../bindings/node"`

`portal:` resolves the directory's `package.json` directly and keeps its
transitive deps. The bindings' package.json `name` fields already match
(`@xmtp/node-bindings` / `@xmtp/wasm-bindings`). Verify each binding
package.json's `dependencies` are minimal during implementation (portal pulls
runtime deps).

`@xmtp/content-type-primitives` stays a published npm dep (`3.0.0`) — content-
types are not migrating.

### `sdks/js/dev/bindings`

Mirrors `sdks/android/dev/bindings`. Nix-builds the **test-utils** binding
variants (100% cache hit from the binding test jobs) and populates
`bindings/<x>/dist` in place (gitignored) so `portal:` resolves a real package:

```bash
source "$(dirname "$0")/.setup"
nix build "${ROOT}#node-bindings-test" --out-link "${ROOT}/bindings/node/.nix-dist"
cp -rL "${ROOT}/bindings/node/.nix-dist/dist/." "${ROOT}/bindings/node/dist/"
nix build "${ROOT}#wasm-bindings-test" --out-link "${ROOT}/bindings/wasm/.nix-dist"
cp -rL "${ROOT}/bindings/wasm/.nix-dist/dist/." "${ROOT}/bindings/wasm/dist/"
```

The napi loader prefers the sibling `bindings_node.<target>.node` in `dist/`, so
no per-platform npm package is fabricated. The wasm package consumes the
`wasm-pack --target web` `dist/` directly. `.nix-dist` and `dist` are gitignored.

### `sdks/js/js.just`

Modeled on `bindings/node/node.just`:

```just
export NIX_DEVSHELL := env("NIX_DEVSHELL", "js")
set shell := ["../../dev/nix-shell"]

install:              yarn install
install-ci:           yarn install --immutable
bindings:             ./dev/bindings
check:                yarn workspaces foreach -A run typecheck
build: bindings       yarn workspaces foreach -A run build
test: bindings        yarn workspace @xmtp/node-sdk run test \
                        && yarn workspace @xmtp/browser-sdk run test
# CI: bindings already cached from test-wasm/test-node; nix build = cache hit.
# extra args (e.g. --shard N/M) forwarded to vitest.
test-node-sdk-ci *args="":    (bindings) ; yarn workspace @xmtp/node-sdk run test {{ args }}
test-browser-sdk-ci *args="": (bindings) ; yarn workspace @xmtp/browser-sdk run test {{ args }}
```

(Exact recipe syntax finalized in implementation; the `*args` passthrough to
vitest is the requirement.)

Root `justfile`: add `mod js 'sdks/js/js.just'`. Formatting is handled by
treefmt (Section 4), so `just js format` is NOT added to the root `format`
fan-out.

The `#js` devShell (`nix/js.nix`) already provides yarn/corepack + Playwright
browsers (`PLAYWRIGHT_BROWSERS_PATH`).

---

## Section 3 — CI: test workflows, sharding, gating

### Why not nix-build the SDK dist

SDK builds are `rollup -c` + `tsc` — seconds, not the bottleneck. `vitest run`
re-transforms from `src`, so a prebuilt `dist` is not reused by tests. The real
cost is the Playwright browser run (per-test 120s timeouts). Nix-building dist
adds FOD `yarn.lock`-hash maintenance for no test speedup. **Decision: do not
nix-build the SDK dist.** The high-leverage lever is **vitest sharding**.

### Reusable test workflows (sharded)

`.github/workflows/test-browser-sdk.yml` and `test-node-sdk.yml`,
`on: workflow_call`, `env: NIX_DEVSHELL: js`, runner
`warp-ubuntu-latest-x64-16x`:

```yaml
jobs:
  test:
    strategy:
      fail-fast: false
      matrix: { shard: [1, 2, 3, 4] }     # browser-sdk: 4; node-sdk: 2 (tunable)
    runs-on: warp-ubuntu-latest-x64-16x
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-nix    # cachix `xmtp` (pull)
      - uses: taiki-e/install-action@just
      - run: just js install-ci
      - run: just js bindings                 # nix build *-test = cache hit
      - run: just backend up
      - run: just js test-browser-sdk-ci -- --shard ${{ matrix.shard }}/4
```

- Start shard counts: browser-sdk = 4 (Playwright, slow), node-sdk = 2 (plain
  node vitest). Tunable on real timing; leave a comment to adjust.
- Each shard runs its own `just backend up` (isolated XMTP docker stack per
  runner — required since tests create identities). Parallel runners → aggregate
  backend bring-up cost does not affect wall-clock.
- browser-sdk tests hit `localhost:5557` (grpc-web proxy); node-sdk hits `5556`
  (gRPC) — both served by `just backend up`.

### Gating in `test.yml`

```yaml
test-node-sdk:
  needs: [detect-changes, test-node]
  if: >-
    !cancelled() && needs.test-node.result != 'failure'
    && needs.detect-changes.outputs.node_sdk == 'true'
  uses: ./.github/workflows/test-node-sdk.yml
  secrets: inherit
test-browser-sdk:
  needs: [detect-changes, test-wasm]
  if: >-
    !cancelled() && needs.test-wasm.result != 'failure'
    && needs.detect-changes.outputs.browser_sdk == 'true'
  uses: ./.github/workflows/test-browser-sdk.yml
  secrets: inherit
```

The `!cancelled() && needs.<binding>.result != 'failure'` guard gives the exact
requested semantics: the SDK job **runs after** the binding test job, **proceeds
if it succeeded OR was skipped** (bindings unchanged = nothing to fail), and
**aborts only if it failed**. Without this guard a skipped `needs` would
dead-skip the SDK job when only SDK code changed.

`detect-changes` filters (new outputs), via `dorny/paths-filter@v4`:

```yaml
node_sdk:
  - 'sdks/js/node-sdk/**'
  - 'sdks/js/package.json'
  - 'sdks/js/yarn.lock'
  - 'sdks/js/.yarnrc.yml'
  - 'bindings/node/**'
  - 'crates/**'
  - 'nix/**'
  - 'flake.lock'
  - '.github/workflows/test-node-sdk*'
browser_sdk:
  - 'sdks/js/browser-sdk/**'
  - 'sdks/js/package.json'
  - 'sdks/js/yarn.lock'
  - 'sdks/js/.yarnrc.yml'
  - 'bindings/wasm/**'
  - 'crates/**'
  - 'nix/**'
  - 'flake.lock'
  - '.github/workflows/test-browser-sdk*'
```

Add `test-node-sdk` + `test-browser-sdk` to the final aggregate `test` job's
`needs:` (blocking, like `test-node`/`test-wasm`). All shards must pass.

### Lint / typecheck

`.github/workflows/lint-js.yml`, `on: workflow_call`, `#js` devShell: runs
`just js bindings` then `eslint` + `tsc` (typecheck). Wired into `lint.yml`'s
fan-out (mirrors the existing `lint-node.yml`/`lint-wasm.yml`). Not sharded; not
gated on bindings beyond needing the `.d.ts`. Prettier formatting is NOT here —
it is in treefmt (Section 4).

---

## Section 4 — Unified formatting + xmtp-js teardown & release sequencing

### Unified formatting via treefmt

xmtp-js formatting is root-level `prettier -w .` (config `.prettierrc.cjs`).
Absorb it into the existing treefmt config (`nix/fmt.nix`, already
`flakeFormatter = true` + `flakeCheck = true`) by adding a `prettier` program
scoped to `sdks/js/**`, porting the `.prettierrc.cjs` settings:

```nix
programs.prettier = {
  enable = true;
  includes = [ "sdks/js/**/*.ts" "sdks/js/**/*.js"
               "sdks/js/**/*.json" "sdks/js/**/*.md" ];
  # settings ported from xmtp-js .prettierrc.cjs
};
```

Then `nix fmt` (and root `just format` → `nix fmt`) formats the SDKs alongside
rust/nix/toml/etc., and `nix flake check` / `just lint-treefmt`
(`nix fmt -- --fail-on-change`, already in `just lint-config`) enforces it.
**One unified formatting config, no separate JS prettier invocation.**

Type-aware lint (`eslint`, flat config) and typecheck (`tsc`) stay in
`lint-js.yml` (Section 3) because they need built bindings + `node_modules` and
cannot run in flake check's pure sandbox.

### Release machinery (libxmtp)

`.github/workflows/release-browser-sdk.yml` + `release-node-sdk.yml`, modeled on
the existing `release-node.yml` / `release-wasm.yml` + `npm-publish.yml`:

1. `nix build` the **publishable** binding variant (`.#node-bindings-js` /
   `.#wasm-bindings`) — what ships to npm, not the test-utils variant.
2. Build the SDK in the `#js` devShell.
3. `npm publish --provenance` with the appropriate dist-tag.

Nightly piggybacks libxmtp's existing bindings nightly cadence: when the
bindings publish, the SDKs publish `--tag nightly` at the matching version. This
replaces xmtp-js's Renovate-driven "bindings npm bump → auto-prerelease" trigger
(the bindings are now local source, so there is no npm bump event).

### Sequencing (the flag-day guard)

Consumers cannot point at npm versions that don't exist yet, so:

- **Phase A — libxmtp import + wire** (Sections 1–3): SDKs build + test in
  libxmtp.
- **Phase B — libxmtp release machinery** (this section): first publish of
  `@xmtp/browser-sdk` / `@xmtp/node-sdk` from libxmtp. npm now has the versions.
- **Phase C — xmtp-js teardown** (separate repo, separate PR, AFTER Phase B
  publishes):
  1. `rm -rf sdks/browser-sdk sdks/node-sdk`, their
     `.github/workflows/{browser,node}-sdk.yml`, their changeset entries, and the
     `release.yml` jobs referencing them. Drop the auto-prerelease-bindings jobs
     for browser/node-sdk (that automation now lives in libxmtp). Keep agent-sdk
     handling.
  2. Rewire consumers to published npm:
     - `apps/xmtp.chat`: `@xmtp/browser-sdk: workspace:^` → `^7.x` published.
     - `sdks/agent-sdk`: `@xmtp/node-sdk: 6.0.0` (workspace-resolved) → `^6.x`
       published.
     - `packages/xmtp-cli`: `@xmtp/node-sdk: 6.0.0` → `^6.x` published.
     - content-types: already published `@xmtp/node-sdk@4.6.0` devDep → bump to
       current published; no structural change.
  3. `yarn` to regenerate the lockfile; verify agent-sdk / xmtp-cli / xmtp.chat
     build against published SDKs.
  4. Update xmtp-js Renovate to also track `@xmtp/node-sdk` /
     `@xmtp/browser-sdk` (now external npm deps).

### Remaining cross-repo coupling

`@xmtp/content-type-primitives` (stays in xmtp-js) directly depends on
`@xmtp/node-bindings` and xmtp-js enforced a version-match invariant with
node-sdk. After migration, node-sdk publishes from libxmtp, so this becomes a
**documented cross-repo version expectation** (Renovate on xmtp-js bumps both to
the same published nightly), not an enforced single-repo CI gate. Document this
in the xmtp-js teardown PR.

---

## Decisions log (from brainstorming)

1. **Binding link:** `workspace:`-style local link, realized as Yarn Berry
   `portal:` because bindings are siblings of the `sdks/js/` workspace root.
2. **Workspace root:** `sdks/js/` subtree (not repo root).
3. **Binding ref mechanism:** `portal:../../../bindings/<x>`, dist populated by
   nix via `dev/bindings`.
4. **CI gating:** `needs:` in the `test.yml` orchestrator with an
   `!cancelled() && result != 'failure'` guard (no `workflow_run` chaining).
5. **Binding build variant for tests:** test-utils (`.#*-bindings-test`) — 100%
   cache hit from the binding test jobs.
6. **Scope:** both repos (libxmtp import + xmtp-js teardown), strictly sequenced
   A→B→C.
7. **Release:** mirror libxmtp's existing nix `release-*` + `npm-publish.yml`
   machinery; piggyback the bindings nightly cadence.
8. **CI speedup:** vitest `--shard` across a matrix (not nix-building the SDK
   dist; not offline-cache for now).
9. **Formatting:** unified into treefmt (`nix/fmt.nix` prettier program);
   eslint + tsc stay in `lint-js.yml`.
