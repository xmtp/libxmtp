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
`.agents/skills/checking-spec-compliance/SKILL.md` to find requirements and
evidence. The verdict rules below govern this check.

1. Start with changed behavior and its callers, wire types, and stored state.
   Find the owning specs with `docs/specs/README.md`. Check cross-cutting
   requirements in `AUTH`, `API`, and `PROC` when they apply.
2. Compare code with approved requirements on the PR base branch. A legacy
   obligation can still bind when it has no approved disposition under
   SPEC-013 or when a disposition carries it forward. Cite the exact legacy
   obligation and its mapping before reporting a violation. A draft row alone
   does not establish that mapping. Treat draft specs as proposed contracts.
   Review spec edits in the PR as proposed amendments; do not treat pending
   owner review as a violation by itself.
3. For each affected approved requirement, inspect the changed decision point
   and the assertions in linked `verifies:` tests. Check for deleted assertions,
   disabled cases, and relaxed expectations. A green test result alone does not
   establish compliance. Use PR checks for test results; an unavailable result
   is unknown, not a violation. Do not require this review agent to run tests.
4. Before asking for a new requirement, apply the admission tests in SPEC-020
   through SPEC-023, SPEC-088, and SPEC-092. Name the party that relies on the
   promise and the harm from breaking it. SPEC-072 governs obligations written
   in a spec; it does not require an approved spec for every new code behavior.
   Do not ask for a spec change for an internal implementation choice.
   Leave spec format and wording issues to `just spec-check` and owner review.

Report a blocker only when the PR:

- Causes a confirmed violation of an approved base-branch behavior requirement
  or an exact legacy obligation that still binds.
- Removes the only valid proof of an affected approved requirement, with no
  replacement or valid waiver; or changes an approved spec to promise behavior
  that the code does not provide and no gap waiver records.
- Demonstrably causes data loss, wrong message ordering, a security or privacy
  breach, or incompatibility with supported client or backend versions or stored
  data, even when the owning spec is draft.

For each blocker, give the requirement ID or exact obligation when one exists,
the changed file and line, the base and new behavior, the affected party, the
observable harm, and the required fix. Give a concrete input, output, or state
transition for a blocker without an approved requirement. Report draft-only
disagreements, possible new requirements, pending owner review, old or waived
gaps, extra boundary-test ideas, and unresolved scope as advisory notes in the
summary. Do not turn those notes into blocking findings. If there is no blocker,
say "No blocking violation found within the inspected scope" and pass the
check. Do not claim that tests ran unless their results are available in the PR
checks.
