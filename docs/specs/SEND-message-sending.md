---
prefix: SEND
status: draft
---
# Message sending

The path from an application call to a published envelope. An application needs to know what it can observe after a send fails, and whether retrying can duplicate a message.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_mls/src/groups/intents.rs`, `crates/xmtp_mls/src/groups/mls_sync/publish/`, `crates/xmtp_db/src/encrypted_store/group_intent.rs`. -->

## Scope

In scope: intent creation and queueing, message identifier assignment, the optimistic local record, publish with retry, idempotency against the backend, ordering within a group, what an application observes on failure, and cleanup of intents that can never publish.

Out of scope: publish atomicity on the backend (`API`), the receive path (`PROC`), and commit intents (`GMOD`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
