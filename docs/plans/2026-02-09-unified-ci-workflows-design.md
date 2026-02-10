# Unified CI Workflows Design

## Goal

Replace the current sprawl of independent lint and test GitHub Actions workflows with two parent workflows (`lint.yml` and `test.yml`) that run on every PR and push to main. Each parent detects which areas of the codebase changed and conditionally calls child reusable workflows. A gate job in each parent eliminates the need for `noop.yml`.

## Architecture

```
lint.yml (parent, triggers on every PR + push to main)
├── detect-changes (dorny/paths-filter -> boolean outputs per area)
├── lint-workspace (conditional)
├── lint-node (conditional)
├── lint-wasm (conditional)
├── lint-ios (conditional)
├── lint-android (conditional)
├── lint-toml (conditional)
└── Lint (gate job: always runs, fails if any child failed)

test.yml (parent, triggers on every PR + push to main)
├── detect-changes (dorny/paths-filter -> boolean outputs per area)
├── test-workspace (conditional)
├── test-node (conditional)
├── test-wasm (conditional)
├── test-ios (conditional)
├── test-android (conditional)
├── test-bindings-check (conditional)
└── Test (gate job: always runs, fails if any child failed)
```

## Parent Workflows

### Triggers

Both parents trigger on:

```yaml
on:
  push:
    branches: [main]
  pull_request:
```

No path filters on the parents -- they always run. Path filtering is handled by `detect-changes`.

### Concurrency

Each parent sets concurrency to cancel previous runs for the same PR:

```yaml
concurrency:
  group: lint-${{ github.workflow }}-${{ github.head_ref || github.run_id }}
  cancel-in-progress: true
```

Children inherit cancellation from the parent.

### Permissions

Both parents:

```yaml
permissions:
  contents: read
```

### detect-changes Job

Uses `dorny/paths-filter@v3` to produce boolean outputs. Each output maps to a child workflow.

#### Lint path filters

| Output | Paths |
|---|---|
| `workspace` | `crates/**`, `bindings/**`, `Cargo.toml`, `Cargo.lock`, `.cargo/**`, `rust-toolchain.toml` |
| `node` | `bindings/node/**` |
| `wasm` | `bindings/wasm/**`, `rust-toolchain.toml` |
| `ios` | `sdks/ios/**` |
| `android` | `sdks/android/**`, `bindings/mobile/**` |
| `toml` | `**/*.toml` |

#### Test path filters

| Output | Paths |
|---|---|
| `workspace` | `crates/**`, `bindings/**`, `Cargo.toml`, `Cargo.lock`, `.cargo/**`, `rust-toolchain.toml`, `dev/docker/**`, `dev/up` |
| `node` | `crates/**`, `bindings/node/**`, `Cargo.toml`, `Cargo.lock`, `dev/docker/**`, `.node-version` |
| `wasm` | `crates/**`, `bindings/wasm/**`, `Cargo.toml`, `Cargo.lock`, `nix/**`, `dev/docker/**` |
| `ios` | `sdks/ios/**`, `bindings/mobile/**`, `crates/**`, `nix/**`, `Cargo.toml`, `Cargo.lock` |
| `android` | `sdks/android/**`, `bindings/mobile/**`, `crates/**`, `nix/**`, `Cargo.toml`, `Cargo.lock` |
| `bindings-check` | `bindings/mobile/**`, `Cargo.toml`, `Cargo.lock`, `dev/docker/**` |

### Gate Job

Each parent has a gate job that:

1. Lists all child jobs in `needs:`
2. Uses `if: always()` so it runs even when children are skipped
3. Fails if any child failed or was cancelled

```yaml
lint:
  name: Lint
  runs-on: ubuntu-latest
  if: always()
  needs: [lint-workspace, lint-node, lint-wasm, lint-ios, lint-android, lint-toml]
  steps:
    - run: |
        if [[ "${{ contains(needs.*.result, 'failure') }}" == "true" ||
              "${{ contains(needs.*.result, 'cancelled') }}" == "true" ]]; then
          echo "One or more lint jobs failed or were cancelled"
          exit 1
        fi
```

Branch protection requires the gate job names ("Lint" / "Test"). Since the gate always runs, `noop.yml` is no longer needed.

## Child Workflow Conversions

Each child workflow is converted to a reusable workflow by:

1. Replacing `on: push/pull_request` with `on: workflow_call`
2. Removing path filters (parent handles this)
3. Removing concurrency settings (parent handles this)

All internal job logic (steps, runners, env vars, matrix strategies) stays unchanged.

### Lint children

| Current file | New file | Notes |
|---|---|---|
| `lint-workspace.yml` | `lint-workspace.yml` | Trigger change only |
| `lint-node-bindings.yml` | `lint-node.yml` | Rename + trigger change |
| `lint-wasm-bindings.yml` | `lint-wasm.yml` | Rename + trigger change |
| `lint-ios.yml` | `lint-ios.yml` | Trigger change only |
| `lint-android.yml` | `lint-android.yml` | Trigger change only |
| `lint-toml.yml` | `lint-toml.yml` | Trigger change only |

### Test children

| Current file | New file | Notes |
|---|---|---|
| `test-workspace.yml` | `test-workspace.yml` | Trigger change only |
| `test-node-bindings.yml` | `test-node.yml` | Rename + trigger change |
| `test-webassembly.yml` | `test-wasm.yml` | Rename + trigger change |
| `test-ios.yml` | `test-ios.yml` | Trigger change only (multi-job structure preserved) |
| `test-android.yml` | `test-android.yml` | Trigger change only |
| `check-ios-android-bindings.yml` | `test-bindings-check.yml` | Rename + trigger change |

### Deleted

| File | Reason |
|---|---|
| `noop.yml` | Replaced by gate jobs |

## Post-Deploy

After merging, verify the exact check names GitHub produces for the gate jobs and update branch protection required checks to match.

## What Stays the Same

- All job internals (steps, runners, env vars, matrix strategies)
- All composite actions (`setup-rust`, `setup-node`, `setup-nix`)
- All secret usage within child workflows
- The multi-job deploy/test/cleanup structure in `test-ios.yml`
