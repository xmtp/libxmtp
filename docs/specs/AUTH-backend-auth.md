---
prefix: AUTH
status: draft
---
# Backend auth

Authentication is optional per deployment, so this spec states both what an authenticating backend admits and how a client carries credentials. Getting the terminal cases wrong strands a client in a retry loop it can never escape.

<!-- Skeleton. The Phase 2 author replaces the Scope and Terms text and adds
     the capability sections and their requirements.
     Sources: Legacy 002 auth prose; the API key spec.
     Code anchors: `apps/backend/src/auth/`, `apps/backend/src/server/auth.rs`, `crates/xmtp_api_backend/src/middleware/auth.rs`. -->

## Scope

In scope: admission rules and exempt paths, API keys, JWT verification and scopes, JWKS lifecycle, the client credential callback and refresh, the failure taxonomy and which failures are terminal, and redaction guarantees.

Out of scope: the `[auth]` configuration table (`CONF`) and the status codes themselves (`API`).

## Terms

| Term | Meaning |
| --- | --- |
| | |

## 1. First capability

Prose that explains the mechanism, then its requirements.
