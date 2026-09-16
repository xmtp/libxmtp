# XMTP CLI

Standalone Yarn 4 project for `@xmtp/cli`, linked to the in-tree Node SDK with
`portal:../../sdks/js/node-sdk`.

## Commands

- `just cli install`: install CLI dependencies and update the lockfile.
- `just cli install-ci`: install with the immutable lockfile.
- `just cli check`: build the linked SDK and typecheck the CLI.
- `just cli lint`: run ESLint.
- `just cli build`: build the linked SDK and CLI.
- `just cli test`: build the linked SDK and run all tests.
- `just cli test-ci --shard N/2`: run a CI test shard.

Run every command through `dev/nix-shell`, for example
`dev/nix-shell 'just cli test'`. Tests require `just backend up`. The `sdk`
dependency installs `sdks/js`, stages Node bindings, and builds
`@xmtp/node-sdk` before checks that need its `dist` output.

## Backend configuration

- Commands that create a client or contact a backend require `--backend-url`
  or `XMTP_BACKEND_URL`.
- `--env` and `XMTP_ENV` are database labels only. They do not select a
  network.
- The default database path is derived from the backend origin. Explicit
  `--db-path` or `XMTP_DB_PATH` values take precedence.
