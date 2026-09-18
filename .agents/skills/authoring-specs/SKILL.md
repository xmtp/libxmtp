---
name: authoring-specs
description: Use when drafting a new spec in specs/ or amending an approved one - covers the sources to read, the admission test, the requirement row, precision rules, ID allocation, protobuf and WebIDL type blocks, and the change summary a reviewer needs
---

# Authoring a spec

Read `specs/SPEC-spec-format.md` first: it is the rules, this skill is the order of work. A spec states promises the system keeps, not what the code does today.

## Sources

1. The skeleton for this prefix, `specs/README.md`, and `specs/GLOSSARY.md`.
2. The legacy spec named in the skeleton, the XIPs it cites, and its code anchors. Code is evidence of current behaviour, not authority for intended behaviour; XIPs have no authority of their own. When sources disagree, write the intended behaviour and record the disagreement.
3. The sections of RFC 9420 and of any other standard the legacy spec or the code relies on. You will cite them by section (SPEC-074).
4. `just spec-index`, for obligations that already exist. Reference them by ID; never restate them.

## The requirement row

One obligation per row of a table with the header `| ID | Title | Requirement | Why |`:

```markdown
| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| JOIN-012 | Stale welcome | When a Welcome names a group the client already holds and its epoch is not later than the local epoch, the client MUST discard the Welcome. | An older Welcome would roll a member back to a dead epoch. |
```

Condition first, then the actor, then the keyword. That is the EARS clause order (`when`, `while`, `where`, `if ... then`) written with MUST and MUST NOT instead of SHALL, and lowercase triggers. A trigger buried at the end of a sentence cannot be tested.

- MUST and MUST NOT state obligations; MAY states freedom. Never `SHALL`.
- `SHOULD` only for an app or an operator, which the system cannot enforce. Those need no test.
- The ID cell holds the bare identifier. Two to seven words of title in the Title cell, no period. At most three sentences in the Requirement cell.
- The Why cell says what breaks when the rule is violated, or is empty. It never repeats the rule. Section prose carries the reasons by default.
- A row is one line: no fence, no list. A pipe inside a cell is `\|`. A type block goes above the table whose rows point at it.

## Precision

Write the check, not a description of it. Every vague requirement costs an owner a review comment and you a round trip.

- Name the field, the stored value, and the comparison. "The message with the lowest sequence id at the location", not "the earliest message". "The anchor is greater than the group's stored cursor", not "has not yet read up to". Name the field of the wire type or the MLS object that the check reads.
- A bound states its number, or its default and the rule that chooses it. "A bounded number of attempts" cannot be violated.
- Name the algorithm when one is fixed: "ChaCha20-Poly1305", not "a cipher that authenticates".
- Cite a standard by section, link it, and say only what XMTP adds or removes: "the checks in RFC 9420 §10.1, and in addition ...". Never restate the standard's own validation.
- Say what a thing is and what it carries, not what it intends. A key package is "the public key material and capabilities another member needs to add this installation", not "a standing offer". Use the protocol's term: "expired", "randomly generated topic", "broken group". Say who is trusted for what; never "trustworthy".
- Delete the words that stand in for a measure: earliest, latest, confirmed, usable, bounded, sufficient, appropriate, promptly.

## Writing

- Write a section's prose before its requirements: what the mechanism is and what breaks without it, in a few direct sentences. No argument for the design, no alternatives, no decision history (SPEC-010). A reader who wants a reason has the Why cell.
- Apply the admission test (SPEC §2) to every candidate and reject anything that fails one question. Ask in particular who observes a violation: if only the actor that breaks the rule, it is a design note, not a requirement (SPEC-092).
- State the general invariant, never one example of it. A rule and the value it fixes are one row. Two rows a single act would violate are one row (SPEC-028).
- Allocate the next unused number. Never renumber, never reuse; check `git log -S` when in doubt. An id carried forward from a superseded document, or moved here by a split, goes in the `owns` frontmatter key.
- A protobuf message is inlined verbatim with its field numbers, which are normative. A structure that never reaches the wire is a WebIDL dictionary. A format defined elsewhere, such as an MLS object in TLS encoding, is cited by section, never reproduced (SPEC §3.1).
- An exact wire, limit, or error value goes in the spec that owns it. Others reference the ID.
- When the obligation you need is not written yet, write `?PREFIX` where the id will go and say in the section's prose what it should require, not inside the row. Never invent a number: it will resolve to the wrong requirement once that number is allocated. The checker lists every marker, and the PR approving that spec has to replace them.
- No file names, no phases, no status prose, no platform choices. Mechanism detail belongs in a design note or module README.

## Amending

Same obligation, better words: keep the ID. Different obligation: new ID, delete the old row. Moving a clause out of a requirement into prose is a contract change; say so.

Every obligation in the legacy source needs a disposition in the summary: carried into a requirement, retired, or moved to a design note. A platform choice cannot be carried, so say that it is retired rather than letting it vanish with the file.

## Finish

Run `dev/nix-shell 'just spec-check'`. Return the spec and a change summary: IDs added, amended, removed; legacy requirements not carried, each with a reason; waivers the code will need; questions only an owner can answer. Do not set `status: approved`.
