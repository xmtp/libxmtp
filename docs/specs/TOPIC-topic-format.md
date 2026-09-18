---
prefix: TOPIC
status: draft
---
# Topic format

Every envelope the backend stores is routed by a topic: one kind byte followed by an identifier. This spec owns that layout and nothing else, so the specs that use topics reference it instead of each restating the table.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 001 section 2.
     Code anchors: `crates/xmtp_proto/src/types` (topic), `apps/backend/src/validation.rs`. -->

## Scope

In scope: the topic kinds, the identifier length and derivation source for each kind, and the rule that the backend derives a topic from the payload rather than accepting one from a client.

Out of scope: what the backend does with an envelope once routed, which belongs to `API`, and how a client subscribes to topics, which belongs to `PROC`.

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
