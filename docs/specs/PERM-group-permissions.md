---
prefix: PERM
status: draft
---
# Group permissions

The policy engine that decides whether a proposed change is allowed. It is stated separately from `GMOD` because it is evaluated identically by every client, and because its rules must be able to change without forking older clients.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: None; authored from the code.
     Code anchors: `crates/xmtp_mls/src/groups/group_permissions.rs`, `crates/xmtp_mls/src/groups/app_data/policy.rs`. -->

## Scope

In scope: the policy set structure, preconfigured policies and composition, admin and super-admin semantics, how a commit is evaluated against its proposer, the DM carve-out, and how a new rule is introduced.

Out of scope: where evaluation is invoked (`GMOD`), field write authority as metadata (`META`), and the fixed DM policy's other effects (`DMS`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
