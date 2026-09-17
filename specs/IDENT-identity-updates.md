---
prefix: IDENT
status: draft
---
# Identity updates

An inbox is controlled by a chain of signed identity updates. Every rule here is a security boundary: a mistake lets an attacker attach their key to someone else's inbox, or locks a user out of their own.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: XIP-46.
     Code anchors: `crates/xmtp_id/src/associations/`, `crates/xmtp_mls/src/identity_updates.rs`, `apps/backend/src/validation.rs`. -->

## Scope

In scope: the identity update wire schema and actions, the association state machine, which signature kind may sign which action, inbox id derivation, replay protection, recovery authorization, smart contract wallet verification, and the installation lifecycle.

Out of scope: identity RPCs and the publish rule (`API`), and key package credentials (`JOIN`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
