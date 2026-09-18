---
prefix: API
status: draft
---
# Backend API contract

The gRPC contract every client depends on: what a caller sends, what it gets back, and what the backend guarantees about order, atomicity, and failure. These are the promises that survive any reimplementation of the backend.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: `proto/backend/v1/backend.proto`; legacy 001, legacy 003, legacy 004 sections 1 and 2.
     Code anchors: `apps/backend/src/service/{query,publish,identity}.rs`, `apps/backend/src/stream/session.rs`, `apps/backend/src/validation.rs`. -->

## Scope

In scope: ordering and visibility, publish atomicity and idempotency, query and paging semantics, the subscribe frame contracts, identity lookups, the error-code mapping, and what admission validation does and does not establish.

Out of scope: credentials (`AUTH`), limit values (`CONF`), notification delivery (`PUSH`), the topic layout (`TOPIC`), and what a client does with received frames (`PROC`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
