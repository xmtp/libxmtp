---
title: Spec Compliance
model: gpt-6-luna
reasoning: high
input: full_diff
conclusion: failure
---

## Spec compliance review

Review the full PR for changes to documented behavior. Read `AGENTS.md`,
`docs/self-hosted/agent-context.md`, `docs/specs/README.md`, and the relevant
directory `AGENTS.md` files. Use
`.agents/skills/checking-spec-compliance/SKILL.md` for the review method.

1. Start with changed behavior and its callers, wire types, and stored state.
   Find the owning specs with `docs/specs/README.md`. Check cross-cutting
   requirements in `AUTH`, `API`, and `PROC` when they apply.
2. Compare code with approved requirements on the PR base branch. An obligation
   in a legacy spec stays binding until it has an approved disposition. Treat a
   draft spec as a proposed contract. Review spec edits in the PR as proposed
   amendments; do not use them to excuse a violation of the base contract.
3. For each affected requirement, inspect the changed decision point and the
   assertions in linked `verifies:` tests. Check for deleted assertions,
   disabled cases, relaxed expectations, and missing boundary tests. A green
   test result alone does not establish compliance.
4. Identify a spec change when the PR adds, changes, or removes an observable
   promise that another party relies on. Name the owning spec and affected IDs.
   Identify needed waivers for approved requirements without valid evidence.
   Do not ask for a spec change for an internal implementation choice.

Report confirmed regressions and missing required spec changes as blockers.
For each finding, give the requirement ID or missing contract, file and line,
the observed change, and the required fix. Separate new violations from old
gaps. List any scope you could not resolve. If no violation is found, say
"No violation found within the inspected scope." Do not claim that tests ran
unless their results are available in the PR checks.
