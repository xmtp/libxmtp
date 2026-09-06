<!-- markdownlint-configure-file {"MD029": false} -->
# Self-Hosted Project Guidelines

Hard rules for the self-hosted transition. They override normal repository practice.
They do not override `docs/self-hosted/project.md` or an approved spec in `docs/specs`.
Delete this file when the project ends.

## Branch and build

1. Branch from `origin/self-hosted`. Open every PR into `self-hosted`, as a `gh stack` for a very large phase. Never into `main`.
2. `cargo build` must pass at the end of each phase. Mid-phase it may fail.
3. Failing tests are allowed mid-phase. Do not disable a test to hide a real bug.
4. `just lint` must pass before a PR is opened. Mid-phase commits may skip it.
5. Work in one checkout. Add a worktree only for work that shares no files and has no dependency on other work.

## Deleting code

6. Delete code `docs/self-hosted/deletions.md` marks for deletion as soon as every keeper and dependent has moved off it. It gives the order and the keep list.
7. Delete dead code in the same PR that orphans it. Do not comment it out. Do not deprecate.
8. Delete a test when its behavior no longer exists. Do not add a second test for a requirement ID on the same platform.
9. Never add a compatibility shim for the xmtpd or xmtp-node-go wire formats.

## Architecture

10. The backend is one binary. It scales horizontally behind a load balancer.
11. Durable state lives in Postgres. Instance-local stream state and caches are disposable; reconnect must not depend on them.
12. Code used by both the backend and a client goes in a shared crate. Not in `apps/backend`. Not in `xmtp_mls`.
13. Never copy a function between crates. Move it to the shared crate and import it.
14. Values shared by more than one crate go in `xmtp_configuration`. A constant used by one module stays in that module. Every number has a name.

## Specs and plans

15. Every implementation task needs an approved plan in Ref before you write code.
16. Approved specs go in `docs/specs`. Specs state behavior and errors. Specs name no files.
17. Plans use EARS requirements and may name files, modules, and lines.
18. Phase 2 and later need specs 001 and 002 approved first.

## Tests

19. New Rust tests use `#[xmtp_common::test(unwrap_try = true)]`. Exception: a crate that `xmtp_common` depends on.
20. New `xmtp_mls` tests that need a client use the `tester!` macro from `xmtp_mls` test utils.
21. Every new API endpoint gets an integration test: happy path, each error, each limit.
22. Before adding a test, search `docs/self-hosted/tests/existing-requirements.md` for its requirement ID.

## Docs

23. Agent instructions go in `AGENTS.md`. Put a `CLAUDE.md` next to it holding `@AGENTS.md`.
24. Update the package `AGENTS.md` in the same PR that changes its build or test commands.
25. Ask when a rule here blocks you. Do not work around it.
