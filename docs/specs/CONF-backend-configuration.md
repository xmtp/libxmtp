---
prefix: CONF
status: draft
---
# Backend configuration

An operator configures one deployment in a TOML file; the backend publishes part of that configuration to clients and keeps the rest private. This spec owns the meaning of each option, the public and private boundary, and how a client fetches, stores, and applies what it receives.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 006.
     Code anchors: `apps/backend/src/config.rs`, `apps/backend/src/service/configuration.rs`, `crates/xmtp_mls/src/server_configuration.rs`, `crates/xmtp_configuration/src/common/server.rs`. -->

## Scope

In scope: the field catalogue per table with type, default, and validation; environment resolution; cross-field consistency; what `GetConfiguration` publishes; the client's stored copy, its binding to one backend, refresh, and the latches that stop a client.

Out of scope: the auth mechanism itself (`AUTH`), the RPC shape (`API`), and operational behaviour (`OPS`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
