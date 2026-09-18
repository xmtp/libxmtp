---
prefix: PUSH
status: draft
---
# Push subscriptions and webhooks

A client that is not connected still needs to learn that a message arrived. The backend delivers a notification carrying no message content, and a receiver must be able to prove the notification came from the backend.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 005.
     Code anchors: `apps/backend/src/push/`, `apps/backend/src/service/notification.rs`, `crates/xmtp_mls/src/worker/notifications.rs`. -->

## Scope

In scope: recipient registration and the bearer secret, the delivery channels, subscription sync from the client, the dispatcher and its cursor, sender suppression, webhook signing and receiver verification, retention, and the app-visible surface.

Out of scope: the RPC shapes (`API`), the bearer model's relationship to request auth (`AUTH`), and the push configuration table (`CONF`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
