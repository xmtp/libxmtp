---
prefix: OPS
status: draft
---
# Backend operations

Behaviour an operator or a client observes that is not any single RPC's contract: how long data is kept, when an instance reports itself ready, and what a client sees while an instance drains.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 002 behaviour clauses; `docs/backend-observability.md`.
     Code anchors: `apps/backend/src/server.rs`, `apps/backend/src/telemetry/`, `apps/backend/src/db/`. -->

## Scope

In scope: retention and pruning per envelope kind with their exemptions, readiness and health semantics, what a client observes during shutdown, replica read guarantees as a client sees them, and the metric and span catalogue.

Out of scope: platform and deployment choices, which live in the backend module README; per-RPC guarantees (`API`); retention values (`CONF`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
