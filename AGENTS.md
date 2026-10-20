# libxmtp

Rust workspace. MLS messaging. SDKs: `sdks/{agent,android,browser,ios,node,react-native}`.

## Read first

- `docs/specs/` — approved specs. They win over code, plans, and XIPs. Read the
  ones that cover what you are working on, not all of them;
  `docs/specs/README.md` says which spec owns what.
- `REVIEW.md` — what to weigh and report when you review a change.
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
just test dev                # dev tool tests: dev/tests/run, Fly dashboard, release tools
just lint                    # CI Linux lint and Clippy (no Swift, Android, or RN example lint), plus spec-check
dev/nix-shell 'just lint-js-source' # source Oxlint without generated SDK products
dev/nix-shell 'just lint-rust-source' # Rust manifests, without Clippy
just backend up              # shared services
just backend status          # worktree ports and URLs
just outline <paths...>      # declarations and line ranges
just show <file> <name>      # source of a named symbol
just ci-status <pr>          # CI failures and summary
just format                  # format Rust, Nix, TOML, Python, shell, JS, Kotlin, Swift
just disk                    # free space and target/ size of each worktree
```

`just lint` runs its checks in parallel. Output interleaves; use
`just --jobs 1 lint` for unmixed output. The React Native example is a
standalone pnpm project; check it with `just example-react-native check`.

The shared stack in `dev/docker/compose.yml` contains `db`, `replica`, `backend`,
`anvil`, `toxiproxy`, `tempo`, `prometheus`, and `grafana`.
`just backend db-up` starts only `db` and `replica`, without an image build.
`just backend logs [services...]` shows service logs.
Failed Compose startup retains project logs and container state under
`$RUNNER_TEMP/backend-startup-logs`, or the printed temporary path for local runs.
Use `XMTP_BACKEND_URL=http://127.0.0.1:5050` when local IPv6 forwarding fails.
Ports above are the main checkout's. Every worktree gets its own Compose project,
port block, and backend image tag, so run `just backend status` for the checkout you are in. See the
`working-with-worktrees` skill.
`just test` excludes backend database tests; run them with `just backend test`.

`default-members` = `apps/backend`, `apps/db_tools`, `crates/*`. Other apps: see their `AGENTS.md`.

## Rules

- Branch from `origin/main` and target `main` for new PRs.
- Changes that add, amend, or remove a spec requirement need an approved Ref
  plan before implementation. Routine docs, CI, and tooling changes do not.
- Rust tests use `#[xmtp_common::test(unwrap_try = true)]`, not `#[test]`, unless
  the package documents an exception to avoid a dependency cycle.
- Prove that a new test can fail, in any language. Break the behavior it
  checks, or swap in the weaker implementation (revert the fix, widen a match
  arm, drop the call). Run the test and see it fail. Then restore the code. A
  test that passes either way is not coverage.
- Update the relevant directory `AGENTS.md` when its commands change.
- `CLAUDE.md` is only a pointer (`@AGENTS.md`). Content goes in `AGENTS.md`.
- When a Ref plan is required, describe added or changed public types exposed
  through `sdks/*` in it. If new surface is needed during
  implementation, update the plan and keep going. Default to constants; a
  configuration knob should name the caller that needs a non-default value.
- Before a broad source read, use `just outline <path>`. Use `just show <file>
<name>` for a known symbol. Use `rg` if an outline misses a symbol, and Serena
  for Rust references and types.
- Use the Just recipes for builds and tests. If compact output hides a
  diagnostic, repeat the same recipe with `XMTP_RTK=0`. See `docs/agent-tools.md`.
- For CI results use `just ci-status <pr>` and `just ci-failures <job>`, never a
  raw log fetch. See the `check-ci` skill.
- After changes to the CI failure filter, run
  `dev/nix-shell 'python3.11 -B dev/tests/test_ci_failure_markers.py'`. It
  checks local log fixtures.
- `dev/nix-shell 'just lint-config'` checks Rust and config format with
  treefmt, checks the CI suite declaration, and lints the release tools.
- `dev/nix-shell 'just test dev'` runs the dev tool tests. Dev tests check
  rules and script behavior, not the current shape of a workflow. Review a
  filter edit with `dev/ci-suites explain` or `replay` instead of a path table. `dev/tests/run` runs
  every `test_*.py` in the directories it lists, one process per module, in
  parallel. A new test there needs no recipe edit. The tests check CI
  selection, result gates, action pins, release pushes, cache modes, Android
  emulator startup and teardown, and Docker startup diagnostics with local
  process fixtures. CI runs them in the `test_dev` suite.

## Android release workflow

`just test dev` includes the safe release wiring regression in
`dev/tests/test_android_release.py`; it uses a stub Gradle command and never publishes.

## Test environment

Test recipes set `RUST_MIN_STACK=8 MiB` by default. Start `just backend up db
replica` before `just test`; the recipes set `SQLX_OFFLINE` and the worktree's
`DATABASE_URL`. Use the shared backend unless a test needs a specific
configuration. See `crates/xmtp_mls/AGENTS.md` for the ephemeral backend helper.
