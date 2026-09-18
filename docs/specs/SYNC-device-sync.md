---
prefix: SYNC
status: draft
---
# Device sync

A user's installations exchange preferences over a group only they belong to. Accepting an invitation to that group grants access to the user's own state, so the rule for trusting one is a security boundary.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_mls/src/worker/device_sync/`. -->

## Scope

In scope: the sync conversation's lifecycle, discovery, adding missing installations, the preference payloads, idempotent handling, worker durability, and the trust rule for accepting a sync invitation.

Out of scope: consent semantics (`CONS`), general join rules (`JOIN`), and the archive payload format (`ARCH`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
