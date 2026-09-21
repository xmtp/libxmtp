---
prefix: CONS
status: draft
---
# Consent

Consent decides what a user sees. It is set on more than one installation and merged without coordination, so the merge rule has to be deterministic or two devices disagree about the same conversation.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_db/src/encrypted_store/consent_record.rs`. -->

## Scope

In scope: the consent states, inbox and conversation scope, the merge rule when records conflict, defaults and inherited consent, and how consent gates listing and streaming.

Out of scope: propagation between installations (`SYNC`), DM stitching (`DMS`), and stream filtering mechanics (`PROC`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
