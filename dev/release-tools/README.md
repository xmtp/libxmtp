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

Compute a backend release label. Backend uses `--base-version` for its branch label. SDK and CLI workflows use the frozen Python plan instead of this legacy formatter.

| Flag             | Type                     | Required | Description                                                                 |
| ---------------- | ------------------------ | -------- | --------------------------------------------------------------------------- |
| `--sdk`          | string                   | yes      | SDK name                                                                    |
| `--release-type` | `dev` \| `rc` \| `final` | yes      | Release type                                                                |
| `--rc-number`    | number                   | for `rc` | RC number                                                                   |
| `--base-version` | string                   | no       | Override the manifest base; backend releases use the release branch version |

```bash
pnpm --filter @xmtp/release-tools cli compute-version --sdk libxmtp --release-type dev
pnpm --filter @xmtp/release-tools cli compute-version --sdk libxmtp --release-type rc --rc-number 1
pnpm --filter @xmtp/release-tools cli compute-version --sdk libxmtp --release-type final
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

Create `release/<version>` from a clean checkout, synchronize root Cargo,
Cargo.lock, and all six SDK mirrors and the CLI manifest to that target, preserve reviewed notes,
and scaffold missing notes from the previous stable tags. The command commits
the result and records all SDKs in `docs/release-notes/release-<version>.json`.

| Flag        | Type   | Required | Description                               |
| ----------- | ------ | -------- | ----------------------------------------- |
| `--version` | string | yes      | Shared stable SDK target, such as `8.0.0` |
| `--base`    | string | no       | Base ref (default: `HEAD`)                |

```bash
pnpm --filter @xmtp/release-tools cli create-release-branch --version 8.0.0 --base main
```

For a target change without release branch preparation, use `just version set
8.0.0`. `just version check` rejects drift. SDK release orchestration uses
`dev/sdk_version.py plan` to freeze one source and complete dev/RC/final version,
then materializes it before every build. See
[the release guide](../../docs/create-a-release.md).

## Supported SDKs

The SDK release targets are: `ios`, `android`, `node-sdk`, `browser-sdk`, `agent-sdk`, and `react-native-sdk`. CLI (`cli`) shares the SDK version; `libxmtp` (backend) keeps its independent release-label policy. SDK definitions live in `src/lib/sdk-config.ts`.

## Development

```bash
pnpm --filter @xmtp/release-tools run test          # Run tests (Vitest)
pnpm --filter @xmtp/release-tools run test:watch    # Run tests in watch mode
pnpm --filter @xmtp/release-tools run format        # Format with Oxfmt
pnpm --filter @xmtp/release-tools run format:check  # Check formatting
```

## Nix

Run commands through `dev/nix-shell` when you do not use a `just` recipe.
