---
title: Spec Compliance
model: gpt-6-luna
reasoning: high
input: full_diff
conclusion: failure
---

## Spec compliance review

Catch material contract changes, incorrect implementations of new requirements,
and regressions in existing requirements. This is a focused review of the PR,
not an audit of all spec coverage. A completed review passes unless the evidence
establishes a blocker under the rules below.

Read `AGENTS.md` and relevant directory guidance. Use `.agents/skills/checking-spec-compliance/SKILL.md` to find
requirements and evidence. The scope, evidence threshold, and verdict rules in
this file govern this check; the skill's broader audit checklist is not a list
of reasons to fail it.

### Scope and authority

1. Start with changed behavior, its callers, wire types, and stored state.
   Use `docs/specs/README.md` to locate owning specs, then read their status and
   affected requirements on the PR base branch. Follow cross-cutting requirements
   in AUTH, API, and PROC only when the changed behavior reaches them.
2. Approved base-branch requirements are the binding baseline. For a legacy
   obligation, cite its exact text and show why it still binds under SPEC-013.
   A draft row or a deleted legacy document alone does not establish authority.
3. Read proposed spec edits separately. They do not silently replace the approved
   baseline or excuse a regression. An explicit amendment awaiting owner review
   is not itself a defect. Identify the changed promise for the owner; distinguish
   that decision from an implementation that fails either the existing contract
   or the amendment it claims to implement. If the PR implements a new proposed
   requirement, check that implementation too, and label the requirement proposed.
   Do not demand implementation from a spec-only PR that records future work.
4. Read the affected spec's Known limitations and `docs/specs/waivers.toml`.
   Compare base and head before blaming this PR. An unchanged limitation or gap
   is not a new violation, even when nearby code changes. A waiver is not blanket
   permission to widen a gap or break another requirement. A waiver added in this
   PR is a proposed change for owner review, not proof that a regression is safe.
5. Docs, tooling, CI, refactors, and test-only changes need no invented product
   requirement. If they do not change a product contract or its enforcement,
   finish with no blocker. Leave general bugs and security findings to Correctness
   and Security unless they also meet this check's contract criteria.

### Blocking findings

Report only concrete, material defects introduced or worsened by this PR:

- **Unrecorded contract change.** The implementation adds, removes, or changes an
  externally observable promise beyond the approved spec without a corresponding
  proposed amendment for owner review. Name the affected requirement or exact
  missing promise, the party relying on it, and the harmful consequence. Apply
  SPEC-020 through SPEC-023, SPEC-088, and SPEC-092 before requiring an amendment.
  A new helper, API method, configuration option, or unmapped area alone is not
  evidence that a new requirement is needed.
- **Incorrect new implementation.** The PR implements a new requirement but a
  concrete input or state makes its code violate the promised behavior. Cite the
  requirement and trace the failing path. This includes a PR that claims to close
  a gap while its changed implementation still fails that requirement. A missing
  test or removed waiver alone does not prove the implementation is wrong.
- **Existing contract regression.** A concrete input or state worked under an
  applicable approved requirement or binding legacy obligation at the base, but
  the PR breaks it or widens an existing violation. Draft status does not excuse
  demonstrated product data loss, wrong ordering, a security or privacy breach,
  or a break in supported client, wire, or stored-data compatibility. Show the
  affected caller or data, the base behavior, and the new failure. A hypothetical
  consumer or disagreement with draft wording is not enough.

Material means wrong user-visible results, data loss, wrong ordering, a security
or privacy breach, or broken interoperability that another party can observe.
Do not make stylistic or internal implementation differences blocking.

### Evidence and restraint

For a candidate blocker, inspect the changed decision point and enough callers
to establish that the failing path is reachable. Read relevant test assertions;
green checks or a `verifies:` backlink alone do not prove compliance. Check
removed or relaxed assertions for a hidden contract change, but distinguish loss
of coverage from evidence of wrong behavior. Existing equivalent tests count.

Keep missing backlinks, weak or missing tests, waiver bookkeeping, spec format,
wording, and extra boundary cases non-blocking here. Leave mechanical rules to
`just spec-check` and test quality to Code Quality. These requirements still
apply; this check does not duplicate their gates. A test-only change can still
expose a blocker when it accompanies or conceals a demonstrated behavior defect.

Do not require the review agent to run tests or wait for CI. Use available PR
checks only when a test result helps resolve a candidate defect; pending,
skipped, or unavailable results are unknown, not violations. Do not claim a test
ran without its result.

Use prior discussion to avoid repeating resolved findings. Verify the cited
code, limitation, waiver, or owner decision; an author's assertion or a resolved
thread alone cannot waive a contract. Reopen a finding only with new evidence
that it remains a blocker at the current head. Group one root cause into one
finding, even when several requirements or files are affected.

### Report and verdict

For each blocker, give:

- The category above and requirement ID, exact missing promise, or demonstrated
  supported compatibility contract. State whether the requirement is approved,
  proposed, or legacy.
- The changed file and line, concrete input or state, expected behavior, and
  actual behavior. Explain what changed from the base.
- The affected party, material harm, and smallest correction or spec decision
  needed.

Do not fail the check without that evidence. Put useful advisory notes only in
the check summary, clearly marked non-blocking; do not post them as actionable
review findings. Omit routine reminders, speculative concerns, and unaffected
gaps. Mention unresolved scope only when it materially limits this review.

When a completed review finds no blocker, report success and say
"No blocking violation found within the inspected scope." Advisory notes do not
change that verdict. If the review could not inspect relevant changes, state the
limitation rather than claiming a complete review.
