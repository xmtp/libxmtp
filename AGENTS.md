# libxmtp

Rust workspace. MLS messaging. Bindings: `bindings/{mobile,node,wasm}`. SDKs: `sdks/{agent,android,browser,ios,node}`.

## Read first

- `docs/self-hosted/agent-context.md` — self-hosted project rules.
  Read it once at session start. Do not re-read it.
- `docs/specs/` — approved specs. They win over code, plans, and XIPs. Read the
  ones that cover what you are working on, not all of them;
  `docs/specs/README.md` says which spec owns what.
- For TypeScript and JavaScript work, use the `writing-typescript` skill in
  `.agents/skills/`.

## Instruction scope

This file owns shared workflow rules. Directory `AGENTS.md` files add local
commands, constraints, and exceptions. Read the files along the path to the
code you change. Do not copy parent rules into a child file; keep shared rules
in the nearest common parent.

## Commands

Run commands from the repository root. Prefix every `just` example in this
file or a directory `AGENTS.md` with `dev/nix-shell`, for example
`dev/nix-shell 'just lint'`. Never run `cargo`, `pnpm`, `./gradlew`, or `swift`
outside Nix. Prefer a `just` recipe; for Cargo options without one, use
`dev/nix-shell 'dev/agent-run cargo <command> ...'`. See `docs/agent-tools.md`
for the Codex hook, Serena, and command output details.

```bash
just                         # list recipes
just check                   # check default members
just check crate <name>      # check one crate
just test                    # test default members
just test crate <name>       # test one crate
just test workspace -p <name> <args> # focused nextest options and filters
just lint                    # Rust, config, Markdown, and proto
just backend up              # shared services
just backend status          # worktree ports and URLs
just outline <paths...>      # declarations and line ranges
just show <file> <name>      # source of a named symbol
just ci-status <pr>          # CI failures and summary
```

The shared stack in `dev/docker/compose.yml` contains `db`, `replica`, `backend`,
`anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`.
`just backend db-up` starts only `db` and `replica`, without an image build.
`just backend logs [services...]` shows service logs.
Use `XMTP_BACKEND_URL=http://127.0.0.1:5050` when local IPv6 forwarding fails.
Ports above are the main checkout's. Every worktree gets its own Compose project
and port block, so run `just backend status` for the checkout you are in. See the
`working-with-worktrees` skill.
`just test` excludes backend database tests; run them with `just backend test`.

`default-members` = `apps/backend`, `bindings/*`, `crates/*`. Other apps: see their `AGENTS.md`.

## Rules

- Rust tests use `#[xmtp_common::test(unwrap_try = true)]`, not `#[test]`, unless
  the package documents an exception to avoid a dependency cycle.
- Prove that a new test can fail, in any language. Break the behavior it
  checks, or swap in the weaker implementation (revert the fix, widen a match
  arm, drop the call). Run the test and see it fail. Then restore the code. A
  test that passes either way is not coverage.
- Update the relevant directory `AGENTS.md` when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
- When a Ref plan is required, describe added or changed public types exposed
  through `bindings/*` or `sdks/*` in it. If new surface is needed during
  implementation, update the plan and keep going. Default to constants; a
  configuration knob should name the caller that needs a non-default value.
- Before a broad source read, use `just outline <path>`. Use `just show <file>
  <name>` for a known symbol. Use `rg` if an outline misses a symbol, and Serena
  for Rust references and types.
- Use the Just recipes for builds and tests. If compact output hides a
  diagnostic, repeat the same recipe with `XMTP_RTK=0`. See `docs/agent-tools.md`.
- For CI results use `just ci-status <pr>` and `just ci-failures <job>`, never a
  raw log fetch. See the `check-ci` skill.

## Test environment

Test recipes set `RUST_MIN_STACK=8 MiB` by default. Start `just backend up db
replica` before `just test`; the recipes set `SQLX_OFFLINE` and the worktree's
`DATABASE_URL`. Use the shared backend unless a test needs a specific
configuration. See `crates/xmtp_mls/AGENTS.md` for the ephemeral backend helper.
