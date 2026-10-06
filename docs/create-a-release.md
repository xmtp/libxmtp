# Creating an SDK Release

SDK releases use **Actions > Release** (`release.yml`). The release branch
workflow prepares versions and notes. The Release workflow builds and publishes
packages. Do not edit manifest versions by hand.

## Prepare SDK 8.0 from self-hosted

The iOS, Android, Node, Browser, and Agent manifests already contain `8.0.0`.
Use `keep` for these SDKs. A `major` bump would produce `9.0.0`.
The Rust workspace version is separate. Leave `libxmtp-version` empty to keep
it. The branch label `8.0.0` does not set any manifest version.

1. Merge the release preparation PR into `self-hosted`.
2. Select the `self-hosted` workflow revision in **Actions > Create Release Branch**.
3. Set these inputs:

   | Input | Value |
   | --- | --- |
   | `base-ref` | The approved `self-hosted` commit SHA |
   | `version` | `8.0.0` |
   | `pr-base` | `self-hosted` |
   | `ios-bump` | `keep` |
   | `android-bump` | `keep` |
   | `node-sdk-bump` | `keep` |
   | `browser-sdk-bump` | `keep` |
   | `agent-sdk-bump` | `keep` |
   | `cli-bump` | `none` |
   | `libxmtp-version` | Empty |

4. Run the workflow. It creates `release/8.0.0` and a PR into `self-hosted`.
5. Review the prepared notes and the PR checks before publishing an RC.

The command form is:

```sh
gh workflow run create-release-branch.yml \
  --ref self-hosted \
  -f base-ref=self-hosted \
  -f version=8.0.0 \
  -f pr-base=self-hosted \
  -f ios-bump=keep \
  -f android-bump=keep \
  -f node-sdk-bump=keep \
  -f browser-sdk-bump=keep \
  -f agent-sdk-bump=keep \
  -f cli-bump=none
```

Replace `base-ref=self-hosted` with the approved commit SHA to fix the source
commit. `--ref` selects the workflow revision. `base-ref` selects the source
for the new branch. Use the workflow from `self-hosted` so it includes the
release preparation changes.

## Create another release branch

Each SDK choice has this meaning:

| Choice | Result |
| --- | --- |
| `none` | Exclude the SDK from branch preparation |
| `keep` | Include the SDK and keep its current version |
| `patch`, `minor`, `major` | Include the SDK and apply that version bump |

Select at least one SDK. The tool requires a clean working tree.
The branch name is `release/<version>`. Each selected SDK keeps its own version
track. Set `libxmtp-version` only when the release must also change the Rust
workspace version.

Branch preparation writes `docs/release-notes/release-<version>.json`. This
record contains the source commit, Rust version, and selected SDK versions.
It gives the release PR a file change when all notes and versions are already
prepared.

The PR target defaults to `self-hosted`. Use an explicit `pr-base` for a
maintenance release that targets another branch. Use the same target when
publishing the final release.

## Review release notes

Notes are at `docs/release-notes/<sdk>/<version>.md`. Branch preparation keeps
existing notes. For a missing file, it creates a scaffold using the highest
stable SDK tag below the target version. Dev, nightly, RC, and artifact tags
are excluded. If no stable SDK tag exists, a kept version uses a repository
root commit as the comparison baseline. Review that baseline before accepting
an AI draft.

The notes workflow compares each file with its `previous_release_tag`. This
field can contain a commit SHA when there is no prior SDK tag. The prepared
Agent 8.0 notes name the shared legacy Node release as their comparison
baseline because this repository has no previous stable Agent tag.

The first draft of an empty scaffold is committed to the release branch.
For notes that have content, the workflow proposes changes in a PR from
`ai-release-notes/<version>`. Notes-only pushes do not start another draft.
Review the breaking changes, migration steps, and package requirements.
A human must review notes before a final release.

## Validate and publish an RC

Use the release branch for both the workflow revision and the source input.
Use a positive integer for `rc-number`.

```sh
gh workflow run release.yml \
  --ref release/8.0.0 \
  -f ref=release/8.0.0 \
  -f release-type=rc \
  -f rc-number=1 \
  -f ios=true \
  -f android=true \
  -f node-sdk=true \
  -f browser-sdk=true \
  -f agent-sdk=true \
  -f cli=false \
  -f dry-run=true \
  -f no-merge=true
```

`dry-run=true` builds and previews the selected npm packages. It skips npm
publication and git tags. It skips iOS and Android jobs entirely. It does not
prove registry authorization or mobile publication. Check mobile builds and
installed packages separately. The workflow can send a completion notification
even for a dry run.

Before publication:

- Check CI on the source commit. The RC workflow does not require passing CI.
- Check the npm tarballs, native binaries, browser worker, and WASM assets.
- Check iOS SwiftPM and CocoaPods installation and Android Maven installation.
- Check messaging, storage reopen, and migration against the intended backend.
- Resolve the recorded CocoaPods publish failure before accepting the iOS RC.

Run the same command with `dry-run=false` to publish. Each SDK at `8.0.0`
produces `8.0.0-rc1`. npm uses the `prerelease` tag. It does not replace
`latest`. Publish Node and Agent together so Agent pins the Node version from
that run. If Node is omitted, Agent uses npm `latest`, which must be version
8 or later.

Keep the release branch fixed during a run. After a failure, check which
packages were published. Retry only unfinished SDKs at the same source commit.
Do not overwrite a published package. If the source changes, increment the RC
number. If Node succeeded and Agent failed, run **Release Agent SDK** from the
same release branch with `release-type=rc`, the same `rc-number`, and
`node-sdk-version` set to the published Node RC version. Put fixes on
`self-hosted`, then bring them into the release branch.

## Publish the final release

After RC validation, run **Release** with:

| Input | Value |
| --- | --- |
| Workflow revision and `ref` | The release branch |
| `release-type` | `final` |
| SDK switches | The validated SDKs |
| `pr-base` | The release PR target, normally `self-hosted` |
| `dry-run` | `false` |
| `no-merge` | `true` to leave the PR open; `false` to merge it after publication |

The workflow uses a squash merge and keeps the release branch. A final
release with `no-merge=true` leaves the release PR open. Use this setting for
maintenance releases that must not merge automatically.

## Dev and nightly releases

Dev releases can use any branch. Select `release-type=dev`, the source `ref`,
and the SDK switches. Select the source branch as the workflow revision too.
Dev versions from a branch such as `self-hosted` use `<version>-dev.<sha7>`.
Dev versions from `main` use `<version>-pre.<timestamp>.dev.<sha7>`.
Dev releases publish real packages; they are not a dry run.

The scheduled nightly workflow still releases from `main`. Creating an 8.0
release branch does not change the nightly source. Nightlies use the CI gate
and pending-version calculation. RC and final releases use the SDK manifests.

## SDK tags and package destinations

| SDK | RC tag example | Package destination |
| --- | --- | --- |
| iOS | `ios-8.0.0-rc1` | SwiftPM, CocoaPods `XMTP`, and GitHub release assets |
| Android | `android-8.0.0-rc1` | Maven Central `org.xmtp:android` |
| Node | `node-sdk-8.0.0-rc1` | npm `@xmtp/node-sdk` |
| Browser | `browser-sdk-8.0.0-rc1` | npm `@xmtp/browser-sdk` |
| Agent | `agent-sdk-8.0.0-rc1` | npm `@xmtp/agent-sdk` |
| CLI | `cli-<version>-rc1` | npm `@xmtp/cli`; separate version track |

An iOS binary artifact also has a `libxmtp-ios-<sha7>` tag.
RC versions do not change the release branch's base SDK versions.
