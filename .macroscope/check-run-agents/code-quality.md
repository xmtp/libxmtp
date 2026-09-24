---
title: Code Quality
model: gpt-6-luna
reasoning: medium
input: full_diff
conclusion: neutral
---

## Code quality review

Review maintainability in the changed code and the nearby code it uses. Read
`AGENTS.md` and the relevant directory `AGENTS.md` files. For Rust changes,
use `.agents/skills/writing-rust/SKILL.md` and, when tests change,
`.agents/skills/writing-rust-tests/SKILL.md`. Apply the local SDK and binding
instructions to their code. For TypeScript or JavaScript changes, use
`.agents/skills/writing-typescript/SKILL.md`.

Look for an existing helper before accepting new logic. Name the helper and
its location when a change repeats it. Check whether duplicated behavior
belongs in `xmtp_common`, `xmtp_proto`, `xmtp_configuration`, or
`xmtp_mls_common`; keep bindings as translation layers. Flag repeated tests
only when they prove the same property at the same boundary. Do not demand a
shared abstraction for two short operations with different behavior.

Check typed errors, cancellation and retry behavior, transaction boundaries,
platform behavior, clear names, and tests of changed behavior. Check that
generated files are changed through their source and that comments explain
invariants rather than restate code. Flag dead code left by a change and any
compatibility shim for removed xmtpd or xmtp-node-go wire formats. Focus on
issues that a formatter or lint check will not already explain.

Report actionable findings with file and line, the current cost or risk, and
a specific change. Mark material maintenance or test gaps as major; keep
minor style preferences out of the report. If there are no actionable
findings, say "No code quality findings."
