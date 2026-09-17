---
prefix: GMOD
status: draft
---
# Modifying groups

How a group changes after it exists: who may propose what, what a receiving client accepts, and what happens when a member's key material cannot be fetched. A client that accepts a commit it should have rejected forks the group.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_mls/src/groups/validated_commit.rs`, `crates/xmtp_mls/src/groups/mls_sync/publish.rs`, `crates/xmtp_mls/src/groups/intents.rs`. -->

## Scope

In scope: intents for membership, metadata, and admin changes; pre-send guards; the received-commit validation pipeline; identity fetch and validation for added members; failed installations and expired key packages; and committing another member's proposals.

Out of scope: policy evaluation (`PERM`), the app-data content model (`META`), joining (`JOIN`), and fork detection (`FORK`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
