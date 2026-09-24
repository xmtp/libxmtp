<!-- markdownlint-configure-file {"MD029": false} -->
# Agent Context

Read this once at the start of a session. Do not re-read it.

These are the project rules for the self-hosted transition. They override
normal repository practice, and an approved spec in `docs/specs/` overrides
them. Coding conventions and shared helpers are in the `writing-rust` skill;
test conventions are in `writing-rust-tests`. Both are under `.agents/skills/`.

## 1. Project rules

1. Branch from `origin/self-hosted`. Open every PR into `self-hosted`, as a `gh stack` for a very large phase. Never into `main`.
2. Work in one checkout. Add a worktree only for work that shares no files and has no dependency on other work.
3. Only tasks that add, amend, or remove a requirement in a spec need an approved Ref plan before implementation.
4. Approved specs go in `docs/specs/`, one file per capability, in the format `docs/specs/SPEC-spec-format.md` defines. Specs state behavior and errors, and name no files. The specs they superseded are deleted; their history is in git. Plans may name files, modules, and lines, and carry a clearly marked "Spec changes" section listing the requirement IDs they implement and every ID they add, amend, or remove.
5. When a Ref plan is required, describe any added or changed public type exposed through `bindings/*` or `sdks/*` in it. Default to constants - a configuration knob should name the caller that needs a non-default value.
6. When planning new tasks, call out any conflicts with these rules loudly in the project plan.

## 2. Deleting code

1. Delete dead code in the same PR that orphans it. Do not comment it out. Do not deprecate.
2. Delete a test when its behavior no longer exists.
3. Never add a compatibility shim for the xmtpd or xmtp-node-go wire formats.
