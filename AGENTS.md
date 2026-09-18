# libxmtp

Rust workspace. MLS messaging. Bindings: `bindings/{mobile,node,wasm}`. SDKs: `sdks/{android,ios,js}`.

## Read first

- `docs/self-hosted/agent-context.md` — self-hosted project rules and architecture.
  Read it once at session start. Do not re-read it.
- `docs/specs/` — approved specs. They win over code, plans, and XIPs. Read the
  ones that cover what you are working on, not all of them;
  `docs/specs/README.md` says which spec owns what.

## Instruction scope

This file owns shared workflow rules. Directory `AGENTS.md` files add local
commands, constraints, and exceptions. Read the files along the path to the
code you change. Do not copy parent rules into a child file; keep shared rules
in the nearest common parent.

## Commands

The project Codex configuration wraps shell commands in Nix when its hook is
trusted. Keep the explicit wrapper below as a fallback. Serena provides shared,
read-only Rust navigation for this worktree. Its diagnostics are advisory; keep
focused compiler checks and tests. See `docs/agent-tools.md` for setup and limits.

Every `just` recipe runs inside `nix develop` (`dev/nix-shell`).
Never run `cargo`, `yarn`, `./gradlew`, or `swift` bare. Use `just`, or `dev/nix-shell '<cmd>'`.

`just` is not on your PATH outside the Nix shell, and each shell you get is
fresh. Prefix every call: `dev/nix-shell 'just lint'`, not `just lint`.
Prefer a `just` recipe over a hand-rolled `cargo` line. Unless stated otherwise,
commands in all `AGENTS.md` files run from the repository root. Bare `just ...`
examples are shorthand for `dev/nix-shell 'just ...'`, including in directory
files. For Cargo build/check/clippy/test options without a recipe, use
`dev/nix-shell 'dev/agent-run cargo <command> ...'`.

```bash
# Run each one as dev/nix-shell 'just <recipe> [args]'.
just                    # list recipes, including package-specific modules
just check              # cargo check; default-members only
just check crate <name> # focused crate check
just test               # nextest; default-members and the ci profile
just test crate <name>  # focused crate tests
just test workspace -p <name> <args> # forward nextest options and test filters
just lint               # Rust, config, Markdown, and proto; run before commit
just lint-rust          # Clippy, rustfmt, and Hakari
just lint-config        # configuration and source formatting checks
just lint-markdown      # excludes generated glossary and release changelogs
just lint-proto         # Buf checks the local proto/ schemas
just backend up         # shared services; most integration tests need them
just backend status     # this worktree's identity, ports, and URLs
just outline <paths...> # declarations and line ranges; upstream defaults
just show <file> <name> # source of a named symbol
just agent-test         # helper and navigation tests; no language server
just ci-status <pr>     # failures, then a CI summary
just ci-failures <job>  # filtered failure diagnostics
just ci-annotations <check> # file and line annotations
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
- Update the relevant directory `AGENTS.md` when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
- Public API surface belongs in the plan. When a change adds or alters a type
  exposed through `bindings/*` or `sdks/*`, describe that surface in the Ref plan
  so it is approved with the rest of the work. Default to constants — a
  configuration knob should name the caller that needs a non-default value. If
  new surface turns out to be needed mid-implementation, note it in the plan and
  keep going.
- Before a broad source read, use `just outline <path>`. Use `just show <file>
  <name>` for a named symbol, or read the required range. Skip outlines for tiny
  files and exact-range reads. Outlines can be incomplete; use `rg` when a symbol
  is absent. Use Serena for Rust references and type information.
  For smaller output, use `just outline --no-docs --no-fields --no-attrs <path>`.
- Use the existing Just recipes for builds and tests. The Codex hook enables
  compact output inside supported recipes. Without the hook, set `XMTP_RTK=1`
  explicitly. Do not wrap these recipes in `rtk just` or a generic output filter.
  If a diagnostic is missing, follow any recovery hint or repeat the same recipe
  with `XMTP_RTK=0`. Keep its original arguments and environment. Nextest filtering
  additionally requires `XMTP_RTK_NEXTEST=1`; it is off by default.
- For CI results use `just ci-status <pr>` and `just ci-failures <job>`, never a
  raw log fetch. See the `check-ci` skill.

## Test environment

The test recipes default `RUST_MIN_STACK` to 8 MiB. Debug builds of the mobile
bindings need this stack for nested MLS proposal creation. An explicit
`RUST_MIN_STACK` value overrides the default.

`just test` needs `just backend up db replica` and the shared backend services.
It sets `SQLX_OFFLINE=true` for compilation and `DATABASE_URL` for test runs.
The database URL defaults to this worktree's database; the main checkout uses
`postgres://xmtp:xmtp@localhost:55432/xmtp_backend`. Run `just backend status`.
Use the shared backend by default. Use an ephemeral backend only when a test
needs a specific configuration. The helper and its limits are documented in
`crates/xmtp_mls/AGENTS.md`.
