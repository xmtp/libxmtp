---
name: reviewing-specs
description: Use when reviewing a draft or amended spec in specs/ before an owner sees it - covers the format checks, the admission test, cross-spec contradictions, and sampling requirements against the code
---

# Reviewing a spec

Read `specs/SPEC-spec-format.md`. Review with a clean context, on a different model than the author. Your job is to find what is wrong, not to agree.

## Checks, in order

1. `dev/nix-shell 'just spec-check'` passes. Report what it warns about; the checker catches form, not meaning.
2. **Format.** Section order; one obligation per bullet; condition then actor then MUST or MUST NOT; a two-to-seven-word title; at most three sentences; `SHOULD` only for an app or an operator; no `SHALL`; no file, module, or crate names; no phases; no platform choices.
3. **Admission.** For each requirement, ask the five questions in SPEC §2. Name every requirement that fails, and which question it fails. Watch for the common failures: a tunable with no measurement behind it, an internal data layout, a restatement of what the code happens to do, a requirement about tests, and one example of an invariant the spec already states.
4. **Rationale.** Every section's prose says why the mechanism exists. A requirement whose reason is not evident from that prose needs `Why:`.
5. **Duplication.** The obligation is not already stated in another spec (`just spec-index`). An exact value appears in exactly one spec.
6. **Contradictions.** Read the related specs the Scope table names. List any pair of requirements that cannot both hold.
7. **Pending references.** Every `?PREFIX` marker names a registered prefix and says in prose what the missing obligation should require. A marker naming an approved spec is a blocker: that obligation exists now and the reference must point at it.
8. **Reality.** Sample at least ten requirements, weighted toward security, ordering, and error cases, and check each against the code anchors. A requirement the code does not satisfy is a waiver candidate, not a reason to weaken the requirement. Say which ten you sampled.
9. **Type blocks.** A protobuf block must match the schema field for field, including field numbers, since those are normative. A WebIDL block must describe something that never reaches the wire. A foreign format such as an MLS object must be referenced, not reproduced.
10. **Legacy.** Every obligation in the legacy source needs a disposition: carried, retired, or moved to a design note. Challenge any retirement that drops an exact public value, a security limit, or an error contract.
11. **Unowned obligations.** Definitions in the glossary and a spec's Terms name things; they must not carry obligations. An obligation stated only in a table, a type block, or a definition has no owner, and no requirement to amend or evidence.

## Report

A table: ID or section, severity (blocker, major, minor), the finding, and the suggested fix. Then the requirements you sampled in step 7. Distinguish what is wrong from what you could not verify.

Do not edit the spec. Do not set `status: approved`. If the spec is sound, say so briefly rather than inventing findings.
