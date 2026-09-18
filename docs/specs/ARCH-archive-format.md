---
prefix: ARCH
status: draft
---
# Archive format

The format a client exports for backup and device transfer. An archive outlives the version that wrote it, so the compatibility promise is the whole point of specifying it.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_archive/src/`. -->

## Scope

In scope: container framing and encryption, the element kinds, versioning and the promise that older archives still load, what is excluded, and how an import merges with existing local state.

Out of scope: what the data means (`CONS`, `META`, `CTYPE`) and the transport that carries an archive (`SYNC`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
