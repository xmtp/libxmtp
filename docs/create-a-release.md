# Creating a release

Root `Cargo.toml` (`workspace.package.version`) owns the target final version
for iOS, Android, Node, Browser, Agent, React Native, and CLI. Required CI rejects
SDK and CLI metadata that differs from that target. Do not edit their manifest
versions individually. Backend container labels retain their branch-based
release policy.

## Set the shared target

Run from the repository root:

```sh
dev/nix-shell 'just version set 8.0.0'
dev/nix-shell 'just version check'
```

The setter updates Cargo, its workspace lock entries, npm package versions,
and Android metadata. Workspace dependencies remain workspace references;
packing resolves Agent's and CLI's Node dependency to the same complete SDK version.
Changing the package target does not raise protocol compatibility floors.

**Actions > Release: create branch** takes one `version`, a `base-ref`, and
`pr-base` (normally `main`). It sets the shared target, creates
`release/<version>`, preserves reviewed release notes, and scaffolds missing
notes for all six SDKs and CLI. There are no per-SDK bump choices. The notes use each
SDK's preceding stable tag; prerelease and binary artifact tags are excluded.
Review release notes and migration instructions before publishing a final.

## Build and publish

Use **Actions > Release** (`release.yml`). Select the workflow revision and
source `ref` from the release branch containing the tooling changes. For an RC:

```sh
gh workflow run release.yml \
  --ref release/8.0.0 \
  -f ref=release/8.0.0 \
  -f release-type=rc \
  -f rc-number=5 \
  -f ios=true \
  -f android=true \
  -f node-sdk=true \
  -f browser-sdk=true \
  -f agent-sdk=true \
  -f react-native-sdk=true \
  -f backend=false \
  -f cli=true \
  -f dry-run=true \
  -f no-merge=true
```

The next candidate after the published `8.0.0-rc4` is `8.0.0-rc5`, provided it
is still unused. This change does not republish or retag rc4.

The workflow freezes the source SHA and one complete SDK and CLI version in
`sdk-release-plan`. Every builder materializes that version into Cargo and SDK
metadata before compilation, so native runtime versions agree with their
package versions. Every selected SDK and CLI builds and passes its package and
installed-consumer checks before any selected publisher starts. Select each
SDK or CLI explicitly for dev, RC, and final releases. Any subset is allowed;
there is no aggregate `sdks` toggle. Dry runs exercise selected
mobile jobs too; they publish no packages, GitHub releases, or remote tags and
send no notification.
They do not prove registry authorization.

For example, release only Android from `main`:

```sh
gh workflow run release.yml \
  --ref main \
  -f ref=main \
  -f release-type=dev \
  -f android=true
```

All product selectors (`ios`, `android`, `node-sdk`, `browser-sdk`, `agent-sdk`,
`react-native-sdk`, and `cli`) default to false and work for every release type.
Selecting Agent or CLI also selects Node so the matching exact dependency is
available. A single-product dry run builds and checks only that selection and
its required Node dependency; pass `dry-run=true` to skip publication.

| Kind | Published SDK / CLI version | npm channel |
| --- | --- | --- |
| Dev | `8.0.0-pre.<UTC timestamp>.dev.g<sha7>` | `dev` |
| RC | `8.0.0-rc5` | `prerelease` |
| Final | `8.0.0` | `latest` |

Dev builds use the same timestamp format on every branch. The `g` prefix keeps
numeric-only hashes from becoming invalid SemVer numeric identifiers. Dev and RC
publication never advances npm `latest`. Agent and CLI require the matching published
Node version; there is no fallback to `latest`.

Retain `rcN` for 8.0.0 so candidates sort after the existing `rc4`. If more than
nine candidates are needed, RC number 10 produces `rc9.1`, number 11 produces
`rc9.2`, and so on. Later release lines use `rc.1`, `rc.2`, ..., `rc.10`.
Switching directly from `8.0.0-rc4` to `8.0.0-rc.5` would sort backwards.

After the dry run and release-source CI pass, run with `dry-run=false` to
publish. Validate installs, messaging, storage reopen, and migration against
the intended backend. For the final, select `release-type=final`, omit
`rc-number`, and use the validated candidate's code. Final-version binaries
are rebuilt and checked before publication. `no-merge=true` leaves the release
PR open; otherwise the final workflow squash-merges it into `pr-base` after
successful publication. It keeps the release branch.

## Recover a failed publication

Use GitHub's **Re-run failed jobs** on the original run. The frozen plan,
checked npm tarballs, SwiftPM archive/tag bundle, and Maven publication are
retained for seven days. A retry publishes those checked files instead of
building a different package under an existing version. Matching existing npm
or Maven payloads are accepted; changed payloads fail. Existing iOS archives
and APKs must match byte for byte and are never overwritten.

Do not change the source or RC number during a retry. Changed source needs a
new candidate version. Do not re-run all jobs to mint another dev timestamp
and call it a retry. A release completes only when every selected SDK destination
succeeds. If retained artifacts expire, investigate the existing publications
before attempting another release of that version; prefer a new candidate.

## Distribution and tags

| SDK | Example tag | Destination |
| --- | --- | --- |
| iOS | `ios-8.0.0-rc5` | SwiftPM and versioned XCFramework release asset |
| Android | `android-8.0.0-rc5` | Maven Central `org.xmtp:android`, example APK asset |
| Node | `node-sdk-8.0.0-rc5` | npm `@xmtp/node-sdk` |
| Browser | `browser-sdk-8.0.0-rc5` | npm `@xmtp/browser-sdk` |
| Agent | `agent-sdk-8.0.0-rc5` | npm `@xmtp/agent-sdk` |
| React Native | `react-native-sdk-8.0.0-rc5` | npm `@xmtp/react-native-sdk` |
| CLI | `cli-8.0.0-rc5` | npm `@xmtp/cli` |

Standalone XMTP CocoaPods distribution is retired. Existing CocoaPods releases
remain available; iOS consumers moving forward use SwiftPM. React Native keeps
its own npm-shipped podspec for native integration. SwiftPM's binary archive
uses `libxmtp-ios-<complete-version>` so an RC and final from the same source
cannot replace each other's binaries.

## Backend

Leave every SDK selector and `cli` false for a backend-only run. Backend
RC/final labels come from `release/<version>`, independent of Cargo's embedded build identifier.
For example, the backend RC uses `backend-8.0.0-rc5` and
`ghcr.io/xmtp/backend:8.0.0-rc5`. The release builds Linux amd64 and arm64 images
from the pinned source and verifies their attestations before publication.
Matching retries verify the existing tag and platform digests. A backend dry
run builds both architectures without publishing. Backend release also builds
the xdbg image under the backend version; nothing deploys it or changes the
floating `self-hosted` tag.
