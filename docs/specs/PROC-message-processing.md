---
prefix: PROC
status: draft
---
# Message processing

How a client turns envelopes from the network into durable local state, in order, without losing or double-applying anything. These rules are payload-agnostic: they hold whatever the envelope contains.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 004 section 3 onward.
     Code anchors: `crates/xmtp_mls/src/subscriptions/`, `crates/xmtp_mls/src/groups/mls_sync.rs`, `crates/xmtp_db/src/encrypted_store/refresh_state.rs`. -->

## Scope

In scope: how streams and queries converge on one processor, the durable positions and their invariants, catch-up targets, retry versus terminal rejection, capacity and fairness across topics, and delivery to application streams.

Out of scope: the wire frame contract (`API`), what applying an envelope means per kind (`JOIN`, `GMOD`), and the send path (`SEND`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
