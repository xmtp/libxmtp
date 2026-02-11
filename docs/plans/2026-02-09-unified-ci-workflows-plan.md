# Unified CI Workflows Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace 13 independent lint/test CI workflows with two parent workflows (`lint.yml`, `test.yml`) using the parent-gate pattern to conditionally call reusable child workflows.

**Architecture:** Parent workflows trigger on every PR + push to main. A `detect-changes` job uses `dorny/paths-filter@v3` to set boolean flags per area. Child workflows are called conditionally. A gate job always runs and fails if any child failed, replacing `noop.yml`.

**Tech Stack:** GitHub Actions, dorny/paths-filter@v3, reusable workflows (`workflow_call`)

**Design doc:** `docs/plans/2026-02-09-unified-ci-workflows-design.md`

---

### Task 1: Rename lint child workflows

**Files:**
- Rename: `.github/workflows/lint-node-bindings.yml` → `.github/workflows/lint-node.yml`
- Rename: `.github/workflows/lint-wasm-bindings.yml` → `.github/workflows/lint-wasm.yml`

**Step 1: Rename files with git mv**

```bash
cd /Users/nickmolnar/code/xmtp/libxmtp
git mv .github/workflows/lint-node-bindings.yml .github/workflows/lint-node.yml
git mv .github/workflows/lint-wasm-bindings.yml .github/workflows/lint-wasm.yml
```

**Step 2: Verify renames**

Run: `ls .github/workflows/lint-*.yml`
Expected: `lint-android.yml lint-ios.yml lint-node.yml lint-toml.yml lint-wasm.yml lint-workspace.yml`

---

### Task 2: Convert lint child workflows to reusable

**Files:**
- Modify: `.github/workflows/lint-workspace.yml`
- Modify: `.github/workflows/lint-node.yml`
- Modify: `.github/workflows/lint-wasm.yml`
- Modify: `.github/workflows/lint-ios.yml`
- Modify: `.github/workflows/lint-android.yml`
- Modify: `.github/workflows/lint-toml.yml`

For each file, apply the same pattern:
1. Replace the entire `on:` block (push/pull_request/paths) with `on:\n  workflow_call:`
2. Remove the `concurrency:` block (2-3 lines)
3. Remove job-level `permissions:` blocks if present
4. Keep everything else (env, jobs, steps, runners) unchanged

**Step 1: Convert lint-workspace.yml**

Replace lines 2-21 (`on:` block) and lines 22-24 (`concurrency:` block) so the file starts:

```yaml
name: Lint Workspace
on:
  workflow_call:
env:
  CARGO_TERM_COLOR: always
```

Everything from `env:` onward stays the same.

**Step 2: Convert lint-node.yml**

Replace `on:` block (lines 2-11) and `concurrency:` block (lines 12-14) so the file becomes:

```yaml
name: Lint Node
on:
  workflow_call:
jobs:
```

Also update `name:` from `Lint Node Bindings` to `Lint Node`.

**Step 3: Convert lint-wasm.yml**

Replace `on:` block and `concurrency:` block so the file becomes:

```yaml
name: Lint WASM
on:
  workflow_call:
env:
  CARGO_TERM_COLOR: always
```

Also update `name:` from `Lint WASM Bindings` to `Lint WASM`.

**Step 4: Convert lint-ios.yml**

Replace `on:` block and `concurrency:` block:

```yaml
name: Lint iOS
on:
  workflow_call:
jobs:
```

**Step 5: Convert lint-android.yml**

Replace `on:` block and `concurrency:` block:

```yaml
name: Lint Android
on:
  workflow_call:
jobs:
```

**Step 6: Convert lint-toml.yml**

Replace `on:` block and `concurrency:` block:

```yaml
name: Lint TOML
on:
  workflow_call:
jobs:
```

**Step 7: Verify all lint children are valid YAML**

Run: `for f in .github/workflows/lint-*.yml; do echo "--- $f ---"; head -3 "$f"; done`
Expected: Each file should show `name:`, `on:`, `  workflow_call:`

---

### Task 3: Create lint.yml parent workflow

**Files:**
- Create: `.github/workflows/lint.yml`

**Step 1: Write lint.yml**

```yaml
name: Lint

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  detect-changes:
    runs-on: ubuntu-latest
    outputs:
      workspace: ${{ steps.filter.outputs.workspace }}
      node: ${{ steps.filter.outputs.node }}
      wasm: ${{ steps.filter.outputs.wasm }}
      ios: ${{ steps.filter.outputs.ios }}
      android: ${{ steps.filter.outputs.android }}
      toml: ${{ steps.filter.outputs.toml }}
    steps:
      - uses: actions/checkout@v6
      - uses: dorny/paths-filter@v3
        id: filter
        with:
          filters: |
            workspace:
              - 'crates/**'
              - 'bindings/**'
              - 'apps/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - '.cargo/**'
              - 'rust-toolchain.toml'
              - 'rustfmt.toml'
            node:
              - 'bindings/node/**'
            wasm:
              - 'bindings/wasm/**'
              - 'rust-toolchain.toml'
            ios:
              - 'sdks/ios/**'
            android:
              - 'sdks/android/**'
              - 'bindings/mobile/**'
            toml:
              - '**/*.toml'

  lint-workspace:
    needs: detect-changes
    if: needs.detect-changes.outputs.workspace == 'true'
    uses: ./.github/workflows/lint-workspace.yml
    secrets: inherit

  lint-node:
    needs: detect-changes
    if: needs.detect-changes.outputs.node == 'true'
    uses: ./.github/workflows/lint-node.yml
    secrets: inherit

  lint-wasm:
    needs: detect-changes
    if: needs.detect-changes.outputs.wasm == 'true'
    uses: ./.github/workflows/lint-wasm.yml
    secrets: inherit

  lint-ios:
    needs: detect-changes
    if: needs.detect-changes.outputs.ios == 'true'
    uses: ./.github/workflows/lint-ios.yml
    secrets: inherit

  lint-android:
    needs: detect-changes
    if: needs.detect-changes.outputs.android == 'true'
    uses: ./.github/workflows/lint-android.yml
    secrets: inherit

  lint-toml:
    needs: detect-changes
    if: needs.detect-changes.outputs.toml == 'true'
    uses: ./.github/workflows/lint-toml.yml
    secrets: inherit

  lint:
    name: Lint
    runs-on: ubuntu-latest
    if: always()
    needs:
      - lint-workspace
      - lint-node
      - lint-wasm
      - lint-ios
      - lint-android
      - lint-toml
    steps:
      - run: |
          if [[ "${{ contains(needs.*.result, 'failure') }}" == "true" || \
                "${{ contains(needs.*.result, 'cancelled') }}" == "true" ]]; then
            echo "One or more lint jobs failed or were cancelled"
            exit 1
          fi
```

---

### Task 4: Commit lint unification

**Step 1: Stage and commit**

```bash
git add .github/workflows/lint.yml .github/workflows/lint-*.yml
git status  # verify only lint workflow files are staged
git commit -m "feat(ci): unify lint workflows under parent lint.yml

Convert all lint-*.yml workflows to reusable (workflow_call) and
create a parent lint.yml that detects changes via dorny/paths-filter
and conditionally calls each child. Gate job 'Lint' always runs."
```

---

### Task 5: Rename test child workflows

**Files:**
- Rename: `.github/workflows/test-node-bindings.yml` → `.github/workflows/test-node.yml`
- Rename: `.github/workflows/test-webassembly.yml` → `.github/workflows/test-wasm.yml`
- Rename: `.github/workflows/check-ios-android-bindings.yml` → `.github/workflows/test-bindings-check.yml`

**Step 1: Rename files with git mv**

```bash
git mv .github/workflows/test-node-bindings.yml .github/workflows/test-node.yml
git mv .github/workflows/test-webassembly.yml .github/workflows/test-wasm.yml
git mv .github/workflows/check-ios-android-bindings.yml .github/workflows/test-bindings-check.yml
```

**Step 2: Verify renames**

Run: `ls .github/workflows/test-*.yml`
Expected: `test-android.yml test-bindings-check.yml test-ios.yml test-node.yml test-wasm.yml test-workspace.yml`

---

### Task 6: Convert test child workflows to reusable

**Files:**
- Modify: `.github/workflows/test-workspace.yml`
- Modify: `.github/workflows/test-node.yml`
- Modify: `.github/workflows/test-wasm.yml`
- Modify: `.github/workflows/test-ios.yml`
- Modify: `.github/workflows/test-android.yml`
- Modify: `.github/workflows/test-bindings-check.yml`

Same pattern as lint children: replace `on:` block with `workflow_call`, remove `concurrency:`, remove job-level `permissions:`.

**Step 1: Convert test-workspace.yml**

Replace `on:` block and `concurrency:` block:

```yaml
name: Test Workspace
on:
  workflow_call:
env:
  CARGO_TERM_COLOR: always
```

**Step 2: Convert test-node.yml**

Replace `on:` block and `concurrency:` block, update name:

```yaml
name: Test Node
on:
  workflow_call:
jobs:
```

**Step 3: Convert test-wasm.yml**

Replace `on:` block and `concurrency:` block, update name, and **remove `permissions:` blocks from both jobs** (`wasm-ci` and `wasm-integration`):

```yaml
name: Test WASM
on:
  workflow_call:
env:
  CARGO_TERM_COLOR: always
```

Remove these lines from both jobs:
```yaml
    permissions:
      id-token: write
      contents: read
```

**Step 4: Convert test-ios.yml**

Replace `on:` block and `concurrency:` block, update name:

```yaml
name: Test iOS
on:
  workflow_call:
jobs:
```

The multi-job structure (deploy-backend → tests → cleanup) stays unchanged.

**Step 5: Convert test-android.yml**

Replace `on:` block and `concurrency:` block, update name:

```yaml
name: Test Android
on:
  workflow_call:
jobs:
```

**Step 6: Convert test-bindings-check.yml**

Replace `on:` block and `concurrency:` block, update name, and **remove `permissions:` blocks from both jobs** (`check-swift` and `check-android`):

```yaml
name: Test Bindings Check
on:
  workflow_call:
jobs:
```

Remove these lines from both jobs:
```yaml
    permissions:
      id-token: write
      contents: read
```

**Step 7: Verify all test children are valid YAML**

Run: `for f in .github/workflows/test-*.yml; do echo "--- $f ---"; head -3 "$f"; done`
Expected: Each file should show `name:`, `on:`, `  workflow_call:`

---

### Task 7: Create test.yml parent workflow

**Files:**
- Create: `.github/workflows/test.yml`

**Step 1: Write test.yml**

```yaml
name: Test

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  detect-changes:
    runs-on: ubuntu-latest
    outputs:
      workspace: ${{ steps.filter.outputs.workspace }}
      node: ${{ steps.filter.outputs.node }}
      wasm: ${{ steps.filter.outputs.wasm }}
      ios: ${{ steps.filter.outputs.ios }}
      android: ${{ steps.filter.outputs.android }}
      bindings-check: ${{ steps.filter.outputs.bindings-check }}
    steps:
      - uses: actions/checkout@v6
      - uses: dorny/paths-filter@v3
        id: filter
        with:
          filters: |
            workspace:
              - 'crates/**'
              - 'bindings/**'
              - 'apps/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - '.cargo/**'
              - 'rust-toolchain.toml'
              - 'dev/docker/**'
              - 'dev/up'
            node:
              - 'crates/**'
              - 'bindings/node/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - 'dev/docker/**'
              - 'dev/up'
              - '.node-version'
            wasm:
              - 'crates/**'
              - 'bindings/wasm/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - 'nix/**'
              - 'dev/docker/**'
            ios:
              - 'sdks/ios/**'
              - 'bindings/mobile/**'
              - 'crates/**'
              - 'nix/**'
              - 'Package.swift'
              - 'Cargo.toml'
              - 'Cargo.lock'
            android:
              - 'sdks/android/**'
              - 'bindings/mobile/**'
              - 'crates/**'
              - 'nix/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
            bindings-check:
              - 'bindings/mobile/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - 'dev/docker/**'
              - 'dev/up'
              - 'rust-toolchain.toml'
              - '.cargo/**'

  test-workspace:
    needs: detect-changes
    if: needs.detect-changes.outputs.workspace == 'true'
    uses: ./.github/workflows/test-workspace.yml
    secrets: inherit

  test-node:
    needs: detect-changes
    if: needs.detect-changes.outputs.node == 'true'
    uses: ./.github/workflows/test-node.yml
    secrets: inherit

  test-wasm:
    needs: detect-changes
    if: needs.detect-changes.outputs.wasm == 'true'
    uses: ./.github/workflows/test-wasm.yml
    secrets: inherit

  test-ios:
    needs: detect-changes
    if: needs.detect-changes.outputs.ios == 'true'
    uses: ./.github/workflows/test-ios.yml
    secrets: inherit

  test-android:
    needs: detect-changes
    if: needs.detect-changes.outputs.android == 'true'
    uses: ./.github/workflows/test-android.yml
    secrets: inherit

  test-bindings-check:
    needs: detect-changes
    if: needs.detect-changes.outputs.bindings-check == 'true'
    uses: ./.github/workflows/test-bindings-check.yml
    secrets: inherit

  test:
    name: Test
    runs-on: ubuntu-latest
    if: always()
    needs:
      - test-workspace
      - test-node
      - test-wasm
      - test-ios
      - test-android
      - test-bindings-check
    steps:
      - run: |
          if [[ "${{ contains(needs.*.result, 'failure') }}" == "true" || \
                "${{ contains(needs.*.result, 'cancelled') }}" == "true" ]]; then
            echo "One or more test jobs failed or were cancelled"
            exit 1
          fi
```

---

### Task 8: Commit test unification

**Step 1: Stage and commit**

```bash
git add .github/workflows/test.yml .github/workflows/test-*.yml
git status  # verify only test workflow files are staged
git commit -m "feat(ci): unify test workflows under parent test.yml

Convert all test-*.yml workflows to reusable (workflow_call) and
create a parent test.yml that detects changes via dorny/paths-filter
and conditionally calls each child. Gate job 'Test' always runs.
Renames check-ios-android-bindings.yml to test-bindings-check.yml."
```

---

### Task 9: Delete noop.yml and commit

**Files:**
- Delete: `.github/workflows/noop.yml`

**Step 1: Delete and commit**

```bash
git rm .github/workflows/noop.yml
git commit -m "chore(ci): remove noop.yml

No longer needed - the gate jobs in lint.yml and test.yml always
produce the required 'Lint' and 'Test' checks, even when all
child workflows are skipped."
```

---

### Task 10: Push and submit for review

**Step 1: Push and submit**

```bash
gt submit --stack --draft
```

---

### Task 11: Trigger test changes to verify all path groups

Create a commit that touches one file in each path group to verify all workflows trigger correctly. This should be a trivial change (e.g., add a comment or whitespace) in each area:

**Files to touch (one per path group):**
- `crates/xmtp_mls/src/lib.rs` — triggers workspace lint + workspace/node/wasm/ios/android tests
- `bindings/node/package.json` — triggers node lint
- `bindings/wasm/package.json` — triggers wasm lint
- `sdks/ios/README.md` — triggers ios lint
- `sdks/android/README.md` — triggers android lint
- `Cargo.toml` — triggers toml lint + bindings-check test
- `bindings/mobile/Cargo.toml` — triggers android lint + bindings-check test

**Step 1: Make trivial changes**

Add a trailing newline or whitespace-only comment to each file.

**Step 2: Commit and push**

```bash
git add -A
git commit -m "test: trigger all CI path groups for verification"
gt submit --stack --draft
```

**Step 3: Monitor GitHub Actions**

Verify in the GitHub PR:
1. Both `Lint` and `Test` parent workflows appear
2. `detect-changes` job runs and outputs correct flags
3. All expected child workflows are triggered
4. Gate jobs pass (or fail only due to legitimate child failures)
5. Note the exact check names for branch protection update

**Step 4: Revert verification commit**

```bash
git revert HEAD --no-edit
gt submit --stack --draft
```

---

### Task 12: Post-merge — update branch protection (manual)

After merging to main:
1. Go to GitHub repo Settings → Branches → Branch protection rules
2. Update required status checks to match the new gate job names (likely `Lint / lint` and `Test / test`)
3. Remove any old check names that no longer exist
