# Creating a Release

## TL;DR

- **Never manually edit manifest version fields** (`Cargo.toml`, `package.json`, `XMTP.podspec`). The release tooling handles versioning.
- **Always review release notes** before publishing a final release. AI drafts them automatically, but a human must verify before shipping.
- In `main` the version you see in manifests will always be the previously released version. In release branches, it will always be the upcoming version.
- All releases (dev, rc, final) are published through a single workflow: **Actions > Release** (`release.yml`).

---

## Dev Releases

Dev releases can be created from **any branch**. They append `-dev.<commit_hash>` to the manifest version automatically.

1. Go to **Actions > Release** (`release.yml`)
2. Fill in the inputs:

   | Input | Description |
   | ------- | ------------- |
   | `release-type` | `dev` |
   | `ref` | Branch to release from (defaults to the branch you trigger from) |
   | `ios` | Check to release iOS SDK |
   | `android` | Check to release Android SDK |
   | `node` | Check to release Node bindings |
   | `wasm` | Check to release WASM bindings |

3. Click **Run workflow**

A Slack notification is sent to `#notify-dev-releases` when complete.

Dev releases are always drafts, and any pushed changes in the dev release (for example, updating manifests to match the release version) happen in a detached HEAD.

### Consumer bump PRs

A dev release also opens (or updates) a version-bump PR in the `xmtplabs` consumer repos — `convos-ios`, `convos-client`, `convos-cli`, and `convos-assistants` — from the `open-consumer-prs` job, driven by the updatecli manifests in `dev/updatecli/`.

- Each consumer is its own matrix leg, so a red leg never blocks or rolls back the SDK publishes — they already happened. It does turn the run red on purpose: that is the only signal that a bump was not applied. It usually means that consumer's pin format drifted; fix the matching `dev/updatecli/<consumer>.yaml`.
- One PR per libxmtp source branch. Re-releasing from the same branch updates that PR in place, and overlapping runs from the same branch are serialized by the job's concurrency group so the newest release wins.
- Auth comes from the `xmtp-dev-release` GitHub App via the `CONSUMER_PR_APP_ID` and `CONSUMER_PR_APP_PRIVATE_KEY` secrets.
- The job checks the manifests out from `main`, not from the branch being released, because it runs them with a token that can write to the consumer repos. **Manifest changes therefore only take effect once merged to `main`** — releasing from a branch runs `main`'s manifests against that branch's version. Verify manifest edits with the local dry-run below.
- To dry-run a manifest locally (read-only, opens nothing):

  ```bash
  UPDATECLI_GITHUB_TOKEN=$(gh auth token) VERSION=<v> BINDING_VERSION=<v> SOURCE_BRANCH=<branch> updatecli pipeline diff --config dev/updatecli/<consumer>.yaml
  ```

---

## Final Releases

Final releases go through three phases: **create branch → publish RC → publish final**.

### 1. Create a release branch

1. Go to **Actions > Create Release Branch** (`create-release-branch.yml`)
2. Fill in the inputs:

   | Input | Description |
   | ------- | ------------- |
   | `base-ref` | Starting point — commit or branch (default: `main`) |
   | `version` | Release version number, e.g. `1.8.0` |
   | `ios-bump` | Version bump for iOS SDK: `none`, `patch`, `minor`, or `major` |
   | `android-bump` | Version bump for Android SDK: `none`, `patch`, `minor`, or `major` |
   | `node-sdk-bump` | Version bump for Node SDK: `none`, `patch`, `minor`, or `major` |
   | `browser-sdk-bump` | Version bump for Browser SDK: `none`, `patch`, `minor`, or `major` |
   | `node` | Include Node bindings in release |
   | `wasm` | Include WASM bindings in release |

3. Click **Run workflow**

This creates a `release/<version>` branch and opens a PR to `main`.

### 2. Review and edit release notes

Release notes are generated automatically by AI on every push to a `release/**` branch.

- Notes live at `docs/release-notes/<sdk>/<version>.md`
- **First push**: Claude drafts the notes and commits them directly to the release branch
- **Subsequent pushes**: Claude suggests edits via a PR from `ai-release-notes/<version>` into the release branch

To manually edit notes, push changes directly to the release branch. The AI will review your edits on the next push and suggest improvements via PR (which you can accept or ignore).

### 3. (Optional) Publish a Release Candidate

1. Go to **Actions > Release** (`release.yml`)
2. Fill in the inputs:

   | Input | Description |
   | ------- | ------------- |
   | `release-type` | `rc` |
   | `ref` | The `release/<version>` branch |
   | `rc-number` | RC number (e.g. `1`, `2`) |
   | `ios` | Check to release iOS SDK |
   | `android` | Check to release Android SDK |
   | `node` | Check to release Node bindings |
   | `wasm` | Check to release WASM bindings |

3. Click **Run workflow**

RC versions are published as `<version>-rc<number>` (e.g. `4.9.0-rc1`).

### 4. Publish the final release

Once the RC is validated:

1. Go to **Actions > Release** (`release.yml`)
2. Fill in the inputs:

   | Input | Description |
   | ------- | ------------- |
   | `release-type` | `final` |
   | `ref` | The `release/<version>` branch |
   | `ios` | Check to release iOS SDK |
   | `android` | Check to release Android SDK |
   | `node` | Check to release Node bindings |
   | `wasm` | Check to release WASM bindings |
   | `no-merge` | Check to skip auto-merging the release PR to main |

3. Click **Run workflow**

On success, the release PR is automatically merged to `main` and the branch is deleted (unless `no-merge` is checked).

Use the `--no-merge` flag if you are creating a patch to a previous major/minor release.

---

## Branches and Tags

### Branch patterns

| Pattern | Allowed release types |
| --------- | ---------------------- |
| `release/<major>.<minor>.<patch>` | RC, Final |
| `*` (any other branch) | Dev only |

### Tag formats

| SDK | Tag format | Example |
| ----- | ----------- | --------- |
| iOS (final) | `ios-<version>` | `ios-4.9.0` |
| iOS (artifact) | `libxmtp-ios-<sha7>` | `libxmtp-ios-b8bed44` |
| Android | `android-<version>` | `android-5.1.0` |
| Node | `node-bindings-<version>` | `node-bindings-1.10.0` |
| WASM | `wasm-bindings-<version>` | `wasm-bindings-1.10.0` |
