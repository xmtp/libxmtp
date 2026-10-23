# Development commands

Run recipes from the repository root through `dev/nix-shell`. Discover the full
catalog with `dev/nix-shell 'just --list'`; modules expose their own recipes.

## Tooling and CI changes

- `dev/nix-shell 'just lint-config'`: treefmt, CI declaration, release tools.
- `dev/nix-shell 'just test dev'`: dev scripts, Fly dashboard, release tools.
- After editing the CI failure filter, also run
  `dev/nix-shell 'python3.11 -B dev/tests/test_ci_failure_markers.py'`.
- Review CI selection edits with `dev/ci-suites explain` or `replay` inside Nix;
  see [CI selection](../.github/CI.md). Test rules and script behavior, not the
  current arrangement of workflow files.

`dev/tests/run` discovers `test_*.py` in its listed directories and runs each
module in a separate process. New files there need no recipe edit. The Android
release regression uses a stub Gradle command and never publishes.

## Focused lint

`just lint-js-source` needs no generated SDK products; `just lint-rust-source`
checks Rust manifests without Clippy. `just lint` runs checks in parallel; use
`just --jobs 1 lint` for ordered output. It excludes Swift, Android, and the
standalone React Native example's lint; use each platform's recipes.
`just format` formats the repository. Run all of these through `dev/nix-shell`.

## Services and test environment

[compose.yml](docker/compose.yml) declares the shared services.
`dev/nix-shell 'just backend db-up'` starts only PostgreSQL and its replica,
without an image build. `just backend up` starts the full stack;
`just backend logs [services...]` shows logs. Run both through Nix.
Failed startup retains logs/state under `$RUNNER_TEMP/backend-startup-logs`
or the printed local temporary path.

Test recipes set `RUST_MIN_STACK=8 MiB`, `SQLX_OFFLINE`, and the worktree's
`DATABASE_URL`. Use shared services unless a test needs custom configuration;
see [MLS test fixtures](../crates/xmtp_mls/TESTING.md).
For IPv6 forwarding failures, use the IPv4 address of the port printed by
`dev/nix-shell 'just backend status'`.

Read the [worktree skill](../.agents/skills/working-with-worktrees/SKILL.md)
for port allocation, backend image isolation, cache sharing, and cleanup.
