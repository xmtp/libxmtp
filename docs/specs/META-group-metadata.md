---
prefix: META
status: draft
---
# Group metadata and app data

What a group carries besides its messages: its immutable identity, its mutable settings, and the app-data dictionary that lets applications store their own fields without forking clients that do not understand them.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: XIP-81.
     Code anchors: `crates/xmtp_mls_common/src/app_data/`, `crates/xmtp_mls_common/src/group_mutable_metadata.rs`, `crates/xmtp_mls/src/groups/app_data/`. -->

## Scope

In scope: immutable and mutable metadata, the app-data component key space, standard components, encoding and validation rules, preservation of unknown entries, write-once versus modifiable fields, and disappearing-message settings.

Out of scope: who may write a field (`PERM`), how an update travels (`GMOD`), and message payload encoding (`CTYPE`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
