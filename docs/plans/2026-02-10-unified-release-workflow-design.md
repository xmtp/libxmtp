# Unified Release Workflow

## Goal

Merge `dev-release.yml` and `publish-release.yml` into a single `release.yml` workflow that handles all three release types (dev, rc, final) from one dispatch screen.

## Background

The per-SDK release workflows (`release-ios.yml`, `release-android.yml`, `release-node.yml`, `release-wasm.yml`) already accept all three release types uniformly. The split exists only at the orchestrator layer:

- `dev-release.yml` hardcodes `release-type: dev` and takes an optional `branch`
- `publish-release.yml` accepts `rc` or `final` and takes a required `release-branch` plus `rc-number` and `no-merge`

The differences are small enough that a single workflow is clearer and easier to maintain.

## Design

### Inputs

`release.yml` has a single `workflow_dispatch` trigger:

| Input | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `release-type` | choice: `dev` / `rc` / `final` | yes | - | What kind of release |
| `ref` | string | no | `github.ref` | Branch or commit to release from |
| `rc-number` | number | no | - | RC number (only for rc releases) |
| `ios` | boolean | no | `false` | Release iOS SDK |
| `android` | boolean | no | `false` | Release Android SDK |
| `node` | boolean | no | `false` | Release Node bindings |
| `wasm` | boolean | no | `false` | Release WASM bindings |
| `no-merge` | boolean | no | `false` | Skip merging release PR (only for final) |

### Jobs

**`validate`** - Runs first, gates all SDK jobs.
- For `rc` or `final`: asserts the resolved ref matches `release/*`. Fails with a clear error if not.
- For `dev`: no validation, any branch is allowed.

```yaml
validate:
  runs-on: ubuntu-latest
  steps:
    - name: Validate release branch
      if: inputs.release-type != 'dev'
      env:
        REF: ${{ inputs.ref || github.ref }}
      run: |
        BRANCH="${REF#refs/heads/}"
        if [[ ! "$BRANCH" =~ ^release/ ]]; then
          echo "::error::RC and final releases must be run from a release/* branch, got: $BRANCH"
          exit 1
        fi
```

**Per-SDK release jobs** (`release-ios`, `release-android`, `release-node`, `release-wasm`):
- `needs: [validate]`
- Each gated by its boolean input (`if: inputs.ios`, etc.)
- Calls the existing reusable workflow
- Passes: `release-type`, `rc-number`, `ref: ${{ inputs.ref || github.ref }}`
- `secrets: inherit`
- No changes to per-SDK workflows needed.

**`merge-pr`**:
- `needs: [release-ios, release-android, release-node, release-wasm]`
- Condition: `inputs.release-type == 'final' && !inputs.no-merge && !cancelled()` plus no SDK job failed
- Finds the PR from the release branch targeting main and merges it
- Unchanged from current `publish-release.yml`

**`notify`**:
- `needs: [release-ios, release-android, release-node, release-wasm, merge-pr]`
- `if: always()`
- Message: `"${RELEASE_TYPE^} Release:"` followed by each SDK's version/result
- The `${RELEASE_TYPE^}` bash expansion capitalizes the first letter dynamically (Dev/Rc/Final)
- Sends to Slack via `./.github/actions/slack-notify`

### File Changes

| Action | File |
|--------|------|
| Create | `.github/workflows/release.yml` |
| Delete | `.github/workflows/dev-release.yml` |
| Delete | `.github/workflows/publish-release.yml` |
| Update | `docs/create-a-release.md` |

No changes to per-SDK workflows, `npm-publish.yml`, or release tooling.

### Documentation Changes

`docs/create-a-release.md` updates both the Dev Releases and Final Releases sections to reference the single "Actions > Release (`release.yml`)" workflow:

- **Dev Releases**: set `release-type` to `dev`, optionally fill in `ref`, check SDK boxes
- **RC Releases**: set `release-type` to `rc`, fill in the release branch as `ref`, set `rc-number`, check SDK boxes
- **Final Releases**: set `release-type` to `final`, fill in the release branch as `ref`, check SDK boxes
