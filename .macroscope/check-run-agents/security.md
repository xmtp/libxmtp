---
title: Security
model: gpt-6-luna
reasoning: high
input: full_diff
conclusion: failure
---

## Security review

Review changed trust boundaries and security assumptions. Read `AGENTS.md`,
`docs/self-hosted/agent-context.md`, the relevant directory `AGENTS.md` files,
and the owning specs in `docs/specs/`. Follow changed input through validation,
authorization, storage, and output before reporting a risk.

Check for changes that let an unauthenticated or unauthorized actor publish,
read, join, modify, or delete data. Check identity and signature verification,
MLS and key-package validation, group permissions, replay protection, and
error paths that fail open. Check key and secret handling, randomness, transport
authentication, log redaction, data retention, and new dependencies when the
diff touches them. Check that a new API, binding, or SDK path preserves the
same checks as the existing path. In auth callback paths, check that failures
do not retain or log callback error text or credentials. In delivery paths,
check that a message is acknowledged only after the app accepts it and that
stale tokens cannot acknowledge a new delivery.

Report a security finding only when the diff gives a concrete path to harm or
breaks a named assumption. Give the changed file and line, the attacker or
fault precondition, the effect, and the check or fix needed. Mark a confirmed
security regression as a blocker. Separate an unverified concern from a
confirmed finding. If no issue is found, say so without claiming a complete
security proof.
