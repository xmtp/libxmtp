---
prefix: JOIN
status: draft
---
# Joining groups

Everything between publishing a key package and holding usable group state. The rules about when a Welcome may replace existing state are the ones that decide whether a member can still decrypt after a re-add.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: XIP-74.
     Code anchors: `crates/xmtp_mls/src/groups/welcomes/`, `crates/xmtp_mls/src/groups/mls_ext/decrypted_welcome.rs`, `crates/xmtp_id/src/key_package/`, `crates/xmtp_mls/src/worker/key_package_maintenance.rs`. -->

## Scope

In scope: key package format, required extensions, lifetime and rotation, deletion of consumed key material, Welcome unwrapping including the post-quantum wrapper, welcome pointers, when a Welcome may replace local state, and join-time validation.

Out of scope: steady-state commit validation (`GMOD`), association proofs (`IDENT`), DM-specific rules (`DMS`), and consent on join (`CONS`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
