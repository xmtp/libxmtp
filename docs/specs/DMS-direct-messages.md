---
prefix: DMS
status: draft
---
# DMs and stitching

A conversation between two inboxes is an MLS group with a fixed policy. Because either party can create one independently, several groups can exist for the same pair, and clients must agree on which one a user sees.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_db/src/encrypted_store/group/dms.rs`, `crates/xmtp_mls/src/groups/builders.rs`. -->

## Scope

In scope: the identifier derived from the two inboxes, the fixed DM policy, join-time validation, duplicate detection and stitching, which group wins, consent stitching, and deduplication of membership messages across stitched groups.

Out of scope: general join rules (`JOIN`), the policy engine (`PERM`), and consent semantics (`CONS`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
