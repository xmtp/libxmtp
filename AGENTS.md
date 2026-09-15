# libxmtp

Rust workspace. MLS messaging. Bindings: `bindings/{mobile,node,wasm}`. SDKs: `sdks/{android,ios,js}`.

## Read first

- `docs/self-hosted/agent-context.md` — self-hosted project rules and architecture.
  Read it once at session start. Do not re-read it.
- `docs/specs/` — approved specs. They win.
- Skills in `.agents/skills/`. Read one when you start the work it covers:
  `writing-rust` (conventions and shared helpers), `writing-rust-tests`,
  `working-with-nix`, `working-with-worktrees`, `check-ci`.

## Commands

The project Codex configuration wraps shell commands in Nix when its hook is
trusted. Keep the explicit wrapper below as a fallback. Serena provides shared,
read-only Rust navigation for this worktree. Its diagnostics are advisory; keep
focused compiler checks and tests. See `docs/agent-tools.md` for setup and limits.

Every `just` recipe runs inside `nix develop` (`dev/nix-shell`).
Never run `cargo`, `yarn`, `./gradlew`, or `swift` bare. Use `just`, or `dev/nix-shell '<cmd>'`.

`just` is not on your PATH outside the Nix shell, and each shell you get is
fresh. Prefix every call: `dev/nix-shell 'just lint'`, not `just lint`.
Prefer a `just` recipe over a hand-rolled `cargo` line.

```bash
# Run each one as dev/nix-shell 'just <recipe> [args]'.
just                    # list all recipes
just backend up         # docker services. Most tests need them.
just backend build      # self-hosted service through Nix. No database needed.
just backend db-up      # disposable PostgreSQL 18 primary and replica.
just backend status     # this worktree's Compose project, slot, ports, and URLs.
just backend release    # stop this worktree's stack and free its port slot.
just backend sql-prepare # migrate test DB and refresh checked SQL metadata.
just backend sql-check  # verify checked SQL metadata against test DB.
just backend test       # backend unit and RPC/storage tests.
just check              # cargo check. default-members only.
just test               # workspace tests. default-members only.
just lint               # rust + config + markdown. Run before commit.
just lint-markdown      # excludes generated error glossary and JS release changelogs.
just lint-proto         # Buf checks the local proto/ schemas.
just validation         # isolated shared validation checks and native/wasm tests.
just docs build         # Starlight site. Run just docs install first.
just docs lint          # site code and Markdown.
just docs format-check  # site formatting.
just docs test          # site build-tool tests.
just outline <file>     # signature outline of a source file. No bodies.
just ci-status <pr>     # failing CI jobs for a PR, then a one-line summary.
just ci-failures <job>  # why one job failed. Filtered, not the raw log.
just ci-annotations <check>  # file and line annotations for a check run.
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

- Tests use `#[xmtp_common::test(unwrap_try = true)]`. Never `#[test]`.
- Every package has an `AGENTS.md`. Read it before working there. Update it when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
- Public API surface belongs in the plan. When a change adds or alters a type
  exposed through `bindings/*` or `sdks/*`, describe that surface in the Ref plan
  so it is approved with the rest of the work. Default to constants — a
  configuration knob should name the caller that needs a non-default value. If
  new surface turns out to be needed mid-implementation, note it in the plan and
  keep going.
- Read a file's outline before reading the file: `just outline <path>`. Reading
  whole files repeatedly is the largest single source of wasted context.
- For CI results use `just ci-status <pr>` and `just ci-failures <job>`, never a
  raw log fetch. See the `check-ci` skill.

## Ephemeral test backends

`just test` needs `just backend up db replica` and the shared backend services.
It sets `SQLX_OFFLINE=true` for compilation and `DATABASE_URL` for test runs.
The database URL defaults to this worktree's database; the main checkout uses
`postgres://xmtp:xmtp@localhost:55432/xmtp_backend`. Run `just backend status`.
Native `xmtp_mls` tests can use `EphemeralBackend::start(toml)` and
`tester!(alix, backend: &backend)` with optional `auth: callback`.
This helper is available only under `cfg(test)`, not `xmtp_mls/test-utils`.
The backend exports its fixtures through `xmtp_backend/test-utils`. Its own
`cargo test` still uses the existing dev-dependencies.

Use the shared backend on port 5050 by default. Use an ephemeral backend only
when a test needs a specific configuration. Nextest runs each test in its own
process, so tests cannot share an ephemeral backend. Each such test pays for
a database create, a migration, and a listener bind.

Run `dev/check-ephemeral-backend` to check dependency boundaries and the test
recipe environment. The Rust workspace CI job runs this check.
