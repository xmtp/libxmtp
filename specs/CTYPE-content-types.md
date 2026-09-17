---
prefix: CTYPE
status: draft
---
# Content types

How a message payload declares what it is, so that a client which does not understand a type can still show something useful rather than nothing.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: XIP-5.
     Code anchors: `crates/xmtp_content_types/src/`, `proto/message_contents/content.proto`, the three SDK codec registries. -->

## Scope

In scope: the content type identifier scheme and versioning, the encoded-content envelope, the codec contract and its errors, whether a type triggers a notification, nested content, coexistence of legacy and current types, unknown-type handling, and the standard catalogue.

Out of scope: group metadata payloads (`META`) and the device sync payload (`SYNC`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
