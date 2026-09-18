---
prefix: FORK
status: draft
---
# Fork recovery and the commit log

Clients can diverge on group state. The commit log lets a client detect that it has diverged and request repair, which is only sound if entries are never dropped or reordered.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: XIP-68.
     Code anchors: `crates/xmtp_mls/src/groups/commit_log.rs`, `crates/xmtp_mls/src/groups/oneshot.rs`, `crates/xmtp_db/src/encrypted_store/local_commit_log.rs`. -->

## Scope

In scope: writing the local commit log, signing and publishing entries, ingesting remote entries, the fork detection walk and its outcomes, the repair request flow and who may perform it, and the deployment flag that enables the log.

Out of scope: commit validation (`GMOD`), the processing pipeline (`PROC`), the envelope shape (`API`), and the configuration flag's definition (`CONF`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
