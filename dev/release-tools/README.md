# release-tools

A TypeScript CLI for automating SDK release workflows in the libxmtp monorepo — versioning, tagging, release notes, and SPM checksums.

## Setup

**Prerequisites:** Node.js >= 22, Yarn 4

```bash
cd dev/release-tools
yarn install
```

## Usage

```bash
yarn cli <command> [flags]
```

### `find-last-version`

Find the latest published version for an SDK from git tags.

| Flag            | Type    | Required | Description                                  |
| --------------- | ------- | -------- | -------------------------------------------- |
| `--sdk`         | string  | yes      | SDK name (e.g., `ios`)                       |
| `--pre-release` | boolean | no       | Include prerelease versions (default: false) |

```bash
yarn cli find-last-version --sdk ios
yarn cli find-last-version --sdk ios --pre-release
```

### `bump-version`

Bump the version in an SDK manifest (e.g., podspec).

| Flag     | Type                          | Required | Description       |
| -------- | ----------------------------- | -------- | ----------------- |
| `--sdk`  | string                        | yes      | SDK name          |
| `--type` | `major` \| `minor` \| `patch` | yes      | Version bump type |

```bash
yarn cli bump-version --sdk ios --type minor
```

### `compute-version`

Compute a full version string for dev, RC, or final releases. Dev builds append the short git SHA; RC builds append the RC number.

| Flag             | Type                     | Required | Description  |
| ---------------- | ------------------------ | -------- | ------------ |
| `--sdk`          | string                   | yes      | SDK name     |
| `--release-type` | `dev` \| `rc` \| `final` | yes      | Release type |
| `--rc-number`    | number                   | for `rc` | RC number    |

```bash
yarn cli compute-version --sdk ios --release-type dev
yarn cli compute-version --sdk ios --release-type rc --rc-number 1
yarn cli compute-version --sdk ios --release-type final
```

### `update-spm-checksum`

Update the binary target URL and checksum in `Package.swift`.

| Flag         | Type   | Required | Description                      |
| ------------ | ------ | -------- | -------------------------------- |
| `--sdk`      | string | yes      | SDK name                         |
| `--url`      | string | yes      | Artifact download URL            |
| `--checksum` | string | yes      | SHA-256 checksum of the artifact |

```bash
yarn cli update-spm-checksum --sdk ios \
  --url "https://github.com/xmtp/libxmtp/releases/download/ios-1.0.0/LibXMTP.xcframework.zip" \
  --checksum "abc123..."
```

### `scaffold-notes`

Generate a release notes template from git history.

| Flag      | Type   | Required | Description                                                |
| --------- | ------ | -------- | ---------------------------------------------------------- |
| `--sdk`   | string | yes      | SDK name                                                   |
| `--since` | string | no       | Git tag to diff from (defaults to last stable release tag) |

```bash
yarn cli scaffold-notes --sdk ios
yarn cli scaffold-notes --sdk ios --since "ios-1.0.0"
```

### `create-release-branch`

Orchestrate a full release branch — bumps the version, scaffolds release notes, and commits everything.

| Flag        | Type                          | Required | Description                               |
| ----------- | ----------------------------- | -------- | ----------------------------------------- |
| `--sdk`     | string                        | yes      | SDK to bump                               |
| `--version` | string                        | yes      | Release version (used in branch name)     |
| `--bump`    | `major` \| `minor` \| `patch` | yes      | Version bump type                         |
| `--base`    | string                        | no       | Base ref to branch from (default: `HEAD`) |

```bash
yarn cli create-release-branch \
  --version "1.0.0" \
  --base main \
  --sdk ios \
  --bump minor
```

## Supported SDKs

Only `ios` is currently configured. SDK definitions live in `src/lib/sdk-config.ts` — add new entries there to support additional SDKs.

## Development

```bash
yarn test          # Run tests (Vitest)
yarn test:watch    # Run tests in watch mode
yarn format        # Format with Prettier
yarn format:check  # Check formatting
```

## Nix

These tools will be integrated into the Nix devShell for local development soon.
