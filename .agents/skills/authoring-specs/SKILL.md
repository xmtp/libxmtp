---
name: authoring-specs
description: Use when drafting a new spec in specs/ or amending an approved one - covers the sources to read, the admission test, requirement sentence shape, ID allocation, protobuf and WebIDL type blocks, and the change summary a reviewer needs
---

# Authoring a spec

Read `specs/SPEC-spec-format.md` first: it is the rules, this skill is the order of work. A spec states promises the system keeps, not what the code does today.

## Sources

1. The skeleton for this prefix, `specs/README.md`, and `specs/GLOSSARY.md`.
2. The legacy spec named in the skeleton, the XIPs it cites, and its code anchors. Code is evidence of current behaviour, not authority for intended behaviour; XIPs have no authority of their own. When sources disagree, write the intended behaviour and record the disagreement.
3. `just spec-index`, for obligations that already exist. Reference them by ID; never restate them.

## The requirement sentence

One obligation per bullet, in this shape:

```markdown
- **JOIN-012 Stale welcome.** When a Welcome names a group the client already
  holds and its epoch is not later than the local epoch, the client MUST
  discard the Welcome.
  Why: an older Welcome would roll a member back to a dead epoch.
```

Condition first, then the actor, then the keyword. That is the EARS clause order (`when`, `while`, `where`, `if ... then`) written with MUST and MUST NOT instead of SHALL, and lowercase triggers. Keep the order: a trigger buried at the end of a sentence cannot be tested.

- MUST and MUST NOT state obligations; MAY states freedom. Never `SHALL`.
- `SHOULD` only for an app or an operator, which the system cannot enforce. Those need no test.
- A two-to-five-word title, at most three sentences, and `Why:` only when the prose does not already give the reason.

## Writing

- Write a section's prose before its requirements: what the mechanism is for, and what breaks without it. Add a diagram where it explains faster.
- Apply the admission test (SPEC §2) to every candidate; reject anything that fails one question. State the general invariant, never one example of it. A bug fix yields the invariant it violated, or nothing.
- Allocate the next unused number. Never renumber, never reuse; check `git log -S` when in doubt. An id carried forward from a superseded document, or moved here by a split, goes in the `owns` frontmatter key.
- A protobuf message is inlined verbatim with its field numbers, which are normative. A structure that never reaches the wire is a WebIDL dictionary. A format defined elsewhere, such as an MLS object in TLS encoding, is referenced, never reproduced (SPEC §3.1).
- An exact wire, limit, or error value goes in the spec that owns it. Others reference the ID.
- When the obligation you need is not written yet, write `?PREFIX` where the id will go and say in prose what it should require. Never invent a number: it will resolve to the wrong requirement once that number is allocated. The checker lists every marker, and the PR approving that spec has to replace them.
- No file names, no phases, no status prose, no platform choices. Mechanism detail belongs in a design note or module README.

## Amending

Same obligation, better words: keep the ID. Different obligation: new ID, delete the old bullet. Moving a clause out of a requirement into prose is a contract change; say so.

Every obligation in the legacy source needs a disposition in the summary: carried into a requirement, retired, or moved to a design note. A platform choice cannot be carried, so say that it is retired rather than letting it vanish with the file.

## Finish

Run `dev/nix-shell 'just spec-check'`. Return the spec and a change summary: IDs added, amended, removed; legacy requirements not carried, each with a reason; waivers the code will need; questions only an owner can answer. Do not set `status: approved`.
