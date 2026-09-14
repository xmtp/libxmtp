<!-- markdownlint-configure-file {"MD029": false} -->
# Agent Context

Read this once at the start of a session. Do not re-read it.

These are the project rules for the self-hosted transition. They override
normal repository practice, and an approved spec in `docs/specs/` overrides
them. Coding conventions and the shared helpers to reach for are in the
`writing-rust` skill; test conventions are in `writing-rust-tests`. Both are
under `.agents/skills/`. Read a skill when you start the work it covers.

Search for an existing helper before adding one. Move shared code to a shared
crate; do not copy it. Read a package's `AGENTS.md` before working in it -
package-specific conventions belong there.

## 1. Project rules

1. Branch from `origin/self-hosted`. Open every PR into `self-hosted`, as a `gh stack` for a very large phase. Never into `main`.
2. `cargo build` must pass at the end of each phase. Mid-phase it may fail.
3. Failing tests are allowed mid-phase. Do not disable a test to hide a real bug.
4. `just lint` must pass before a PR is opened. Mid-phase commits may skip it.
5. Work in one checkout. Add a worktree only for work that shares no files and has no dependency on other work.
6. Every implementation task needs an approved plan in Ref before you write code.
7. Approved specs go in `docs/specs`. Specs state behavior and errors, and name no files. Plans use EARS requirements and may name files, modules, and lines.
8. Public API surface belongs in the plan. When a change adds or alters a type exposed through `bindings/*` or `sdks/*`, describe that surface in the plan so it is approved with the rest of the work. Default to constants - a configuration knob should name the caller that needs a non-default value.
9. Ask when a rule here blocks you. Do not work around it.

## 2. Deleting code

10. Delete code `docs/self-hosted/deletions.md` marks for deletion as soon as every keeper and dependent has moved off it. It gives the order and the keep list.
11. Delete dead code in the same PR that orphans it. Do not comment it out. Do not deprecate.
12. Delete a test when its behavior no longer exists.
13. Never add a compatibility shim for the xmtpd or xmtp-node-go wire formats.

## 3. Architecture

- The backend is one binary. It scales horizontally behind a load balancer.
- Durable state lives in Postgres. Instance-local stream state and caches are disposable; reconnect must not depend on them.
- Code used by both the backend and a client goes in a shared crate. Not in `apps/backend`. Not in `xmtp_mls`.
- Never copy a function between crates. Move it to the shared crate and import it.
- Values shared by more than one crate go in `xmtp_configuration`. A constant used by one module stays in that module. Every number has a name.
