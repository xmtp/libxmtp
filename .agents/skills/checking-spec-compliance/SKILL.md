---
name: checking-spec-compliance
description: Use when adding implements/verifies backlinks for a spec, or when auditing a PR against the requirements it touches - covers finding affected IDs, the link rules, evidence, waivers, and the report shape
---

# Checking spec compliance

Read `docs/specs/SPEC-spec-format.md` §4 for the link rules. `just spec-index` maps IDs to their links; `just spec-show ID` prints one requirement with its evidence.

## Finding affected requirements

Start from changed behaviour, not changed paths: the callers of what changed, the wire types and tables it touches, and the cross-cutting specs (`AUTH`, `API`, `PROC`). Search the index for those areas. An area the index does not map is unresolved scope; report it as unresolved rather than as no impact.

## Linking

- `implements: ID` in a comment above the item that enforces the obligation. Name the decision point, not every site that participates. None at all when the obligation is a property of the design.
- `verifies: ID` (or several IDs, comma separated) above a test whose assertions establish the obligation. Link a test unless an already-linked test establishes the same property in the same place: four SDK tests of one property at four conversion boundaries all count, and so do the normal-path, rollback, and restart tests of one storage boundary. Prefer the Rust test when two are genuinely redundant. A test that merely exercises the code path does not qualify.
- Never mention a requirement ID anywhere else in a comment. Remove any you find; that is what the checker flags.
- When no test can establish it, add an entry to `docs/specs/waivers.toml` with the ID, a reason, and a `kind`: `analysis` when a recorded review establishes the obligation, or `gap` when the implementation does not yet satisfy it. A `gap` entry names an owner and an issue.

## Auditing a PR

Audit against the approved spec on the base branch, never the PR's edits to it. A spec change inside the PR is a proposed amendment: review it separately, and never let it become the standard the code is judged against.

For each affected ID: read the implementation, read the linked test's assertions, run the test, and look for removed assertions, skipped cases, and relaxed expectations. Green tests can hide a weakened contract. Distinguish a new violation from a pre-existing gap.

## Report

Affected IDs. Per ID, the evidence: test name, result, commit. Findings with the ID and the code location. Unresolved scope. Waivers needed.

Close with "no violation found within inspected scope", never "this cannot break any spec". Do not edit the specs.
