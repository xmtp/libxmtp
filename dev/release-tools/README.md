# release-tools

A TypeScript CLI for automating SDK release workflows in the libxmtp monorepo — versioning, tagging, release notes, and SPM checksums.

## Setup

Use Node.js 26 and pnpm from the Nix shell. Run the commands below with
`dev/nix-shell '<command>'` from the repository root.

```bash
just install
```

## Usage

```bash
pnpm --filter @xmtp/release-tools cli <command> [flags]
```

### `bump-version`

Bump the version in an SDK manifest (e.g., podspec).

| Flag     | Type                          | Required | Description       |
| -------- | ----------------------------- | -------- | ----------------- |
| `--sdk`  | string                        | yes      | SDK name          |
| `--type` | `major` \| `minor` \| `patch` | yes      | Version bump type |

```bash
pnpm --filter @xmtp/release-tools cli bump-version --sdk ios --type minor
```

### `compute-version`

Compute a full version string for dev, RC, or final releases. Dev builds append the short git SHA; RC builds append the RC number.

| Flag             | Type                     | Required | Description  |
| ---------------- | ------------------------ | -------- | ------------ |
| `--sdk`          | string                   | yes      | SDK name     |
| `--release-type` | `dev` \| `rc` \| `final` | yes      | Release type |
| `--rc-number`    | number                   | for `rc` | RC number    |

```bash
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type dev
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type rc --rc-number 1
pnpm --filter @xmtp/release-tools cli compute-version --sdk ios --release-type final
```

### `update-spm-checksum`

Update the binary target URL and checksum in `Package.swift`.

| Flag         | Type   | Required | Description                      |
| ------------ | ------ | -------- | -------------------------------- |
| `--sdk`      | string | yes      | SDK name                         |
| `--url`      | string | yes      | Artifact download URL            |
| `--checksum` | string | yes      | SHA-256 checksum of the artifact |

```bash
pnpm --filter @xmtp/release-tools cli update-spm-checksum --sdk ios \
  --url "https://github.com/xmtp/libxmtp/releases/download/ios-1.0.0/LibXMTP.xcframework.zip" \
  --checksum "abc123..."
```

### `create-release-branch`

Orchestrate a full release branch — bumps versions, scaffolds release notes, and commits everything.

| Flag            | Type                                    | Required | Description                                     |
| --------------- | --------------------------------------- | -------- | ----------------------------------------------- |
| `--version`     | string                                  | yes      | Release version (used in branch name)           |
| `--ios`         | `major` \| `minor` \| `patch` \| `none` | no       | iOS SDK version bump type (default: `none`)     |
| `--android`     | `major` \| `minor` \| `patch` \| `none` | no       | Android SDK version bump type (default: `none`) |
| `--node-sdk`    | `major` \| `minor` \| `patch` \| `none` | no       | Node SDK version bump type (default: `none`)    |
| `--browser-sdk` | `major` \| `minor` \| `patch` \| `none` | no       | Browser SDK version bump type (default: `none`) |
| `--node`        | boolean                                 | no       | Include Node bindings in release                |
| `--wasm`        | boolean                                 | no       | Include WASM bindings in release                |
| `--base`        | string                                  | no       | Base ref to branch from (default: `HEAD`)       |

```bash
pnpm --filter @xmtp/release-tools cli create-release-branch \
  --version "1.0.0" \
  --base self-hosted \
  --ios minor \
  --android patch \
  --node-sdk minor \
  --browser-sdk minor \
  --node \
  --wasm
```

## Supported SDKs

All seven SDKs are configured: `ios`, `android`, `node-bindings`, `wasm-bindings`, `node-sdk`, `browser-sdk`, and `libxmtp`. SDK definitions live in `src/lib/sdk-config.ts`.

## Development

```bash
pnpm --filter @xmtp/release-tools run test          # Run tests (Vitest)
pnpm --filter @xmtp/release-tools run test:watch    # Run tests in watch mode
pnpm --filter @xmtp/release-tools run format        # Format with Oxfmt
pnpm --filter @xmtp/release-tools run format:check  # Check formatting
```

## Nix

Run commands through `dev/nix-shell` when you do not use a `just` recipe.
