# XMTP CLI

pnpm workspace package for `@xmtp/cli`, linked to the in-tree Node SDK.
Resolve `@xmtp/node-bindings` to the local bindings for both the CLI and SDK.
They must share one native module instance for authenticated backends.

## Commands

- `just install`: install the root workspace dependencies.
- `just cli check`: build the linked SDK and typecheck the CLI.
- `just cli lint`: run ESLint.
- `just cli build`: build the linked SDK and CLI.
- `just cli test`: build the linked SDK and run all tests.
- `just cli test-ci --shard N/2`: run a CI test shard.

Tests require `just backend up`. The `sdk` dependency stages Node bindings and builds
`@xmtp/node-sdk` before checks that need its `dist` output.

## Backend configuration

- Use `XMTP_API_KEY` for a static backend bearer token. Do not put keys in argv.
- Commands that create a client or contact a backend require `--backend-url`
  or `XMTP_BACKEND_URL`.
- `--env` and `XMTP_ENV` are database labels only. They do not select a
  network.
- The default database path is derived from the backend origin. Explicit
  `--db-path` or `XMTP_DB_PATH` values take precedence.
