# release-tools

A TypeScript CLI for automating SDK release workflows in the libxmtp monorepo — versioning, tagging, release notes, and SPM checksums.

## Setup

Use Node.js 26 and pnpm from the Nix shell. Run the commands below with
`dev/nix-shell '<command>'` from the repository root.

```bash
just install-js
```

## Usage

```bash
pnpm --filter @xmtp/release-tools cli <command> [flags]
```

This file documents the commands that people run by hand. The release
workflows in `.github/workflows` call the other commands; run
`pnpm --filter @xmtp/release-tools cli --help` to list every command.

### `compute-version`

Compute a full version string for dev, RC, or final releases. Dev builds append the short git SHA; RC builds append the RC number.

| Flag             | Type                     | Required | Description                                                                 |
| ---------------- | ------------------------ | -------- | --------------------------------------------------------------------------- |
| `--sdk`          | string                   | yes      | SDK name                                                                    |
| `--release-type` | `dev` \| `rc` \| `final` | yes      | Release type                                                                |
| `--rc-number`    | number                   | for `rc` | RC number                                                                   |
| `--base-version` | string                   | no       | Override the manifest base; backend releases use the release branch version |

```bash
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type dev
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type rc --rc-number 1
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type final
```

### `publish-backend`

Publish a version manifest at `ghcr.io/xmtp/backend:<version>` and a GitHub
release at `backend-<version>`. Run this after the commit images are published.
The command reads the checked-out source commit. It requires Linux amd64 and
arm64 manifests and refuses a version that belongs to another commit or build.
It verifies signed provenance for both platform digests against `xmtp/libxmtp`,
`push-backend.yml`, and the checked-out commit. Publish the images through the
backend release workflow first. RC versions must use `rcN` with a positive
integer. Dev versions must use `dev.<sha7>` or `pre.<timestamp>.dev.<sha7>`.
Use `--dry-run` to print the plan without contacting registries or GitHub.

```bash
pnpm --filter @xmtp/release-tools cli publish-backend \
  --version 8.0.0-rc1 --release-type rc --dry-run
```

### `create-release-branch`

Create a release branch, prepare selected SDK versions and notes, and commit the result. Use `keep` when an SDK already has the target version. Existing notes are kept. The Rust workspace version changes only with `--libxmtp-version`.

The branch records the source commit and selected versions in `docs/release-notes/release-<version>.json`.

| Flag                | Type                                              | Required | Description                                          |
| ------------------- | ------------------------------------------------- | -------- | ---------------------------------------------------- |
| `--version`         | string                                            | yes      | Release version (used in branch name)                |
| `--ios`             | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | iOS SDK version bump type (default: `none`)          |
| `--android`         | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | Android SDK version bump type (default: `none`)      |
| `--node-sdk`        | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | Node SDK version bump type (default: `none`)         |
| `--browser-sdk`     | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | Browser SDK version bump type (default: `none`)      |
| `--agent-sdk`       | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | Agent SDK selection (default: `none`)                |
| `--cli`             | `major` \| `minor` \| `patch` \| `keep` \| `none` | no       | CLI selection (default: `none`)                      |
| `--libxmtp-version` | string                                            | no       | Explicit Rust workspace version (default: unchanged) |
| `--base`            | string                                            | no       | Base ref to branch from (default: `HEAD`)            |

```bash
pnpm --filter @xmtp/release-tools cli create-release-branch \
  --version "8.0.0" \
  --base main \
  --ios keep \
  --android keep \
  --node-sdk keep \
  --browser-sdk keep \
  --agent-sdk keep
```

## Supported SDKs

The SDK release targets are: `ios`, `android`, `node-sdk`, `browser-sdk`, `agent-sdk`, `cli`, and `libxmtp`. SDK definitions live in `src/lib/sdk-config.ts`.

## Development

```bash
pnpm --filter @xmtp/release-tools run test          # Run tests (Vitest)
pnpm --filter @xmtp/release-tools run test:watch    # Run tests in watch mode
pnpm --filter @xmtp/release-tools run format        # Format with Oxfmt
pnpm --filter @xmtp/release-tools run format:check  # Check formatting
```

## Nix

Run commands through `dev/nix-shell` when you do not use a `just` recipe.
