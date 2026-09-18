# Specifications

The approved specs in this folder are the promises libxmtp and the self-hosted backend keep. `SPEC-spec-format.md` defines the format, the admission test, the identifier scheme, and the backlink rules. `GLOSSARY.md` defines the actors and shared terms. Requirement identifiers are globally unique, such as `CONF-012` or `JOIN-034`.

Specs supersede XIPs and the superseded documents under `docs/legacy-specs/`. A legacy document is source material and is deleted when its replacement is approved.

## Capability map

| Prefix | Spec | Owns | Status |
| --- | --- | --- | --- |
| `SPEC` | [Specification format](SPEC-spec-format.md) | The format, admission, identifiers, backlinks, waivers, plans' spec-change section | approved |
| `TOPIC` | [Topic format](TOPIC-topic-format.md) | The kind byte and identifier layout of every topic | draft |
| `API` | [Backend API contract](API-backend-api.md) | The gRPC contract: ordering, publish, query, subscribe, identity RPCs, error codes, admission validation | draft |
| `CONF` | [Backend configuration](CONF-backend-configuration.md) | The operator's configuration file, its validation, what is published, how the client applies it | draft |
| `AUTH` | [Backend auth](AUTH-backend-auth.md) | Request authentication and authorization, client credentials, terminal auth failures | draft |
| `OPS` | [Backend operations](OPS-backend-operations.md) | Retention and pruning, readiness and health, shutdown as observed, the metric catalogue | draft |
| `PUSH` | [Push subscriptions and webhooks](PUSH-push-subscriptions.md) | Recipient registration, channels, subscription sync, dispatch, HMAC suppression, webhook signing | draft |
| `IDENT` | [Identity updates](IDENT-identity-updates.md) | Association log, signature kinds, inbox id derivation, replay protection, installation lifecycle | draft |
| `JOIN` | [Joining groups](JOIN-joining-groups.md) | Key packages, Welcomes, welcome pointers, when a Welcome replaces local state, join validation | draft |
| `GMOD` | [Modifying groups](GMOD-modifying-groups.md) | Commits and proposals, commit validation, adding members, failed installations | draft |
| `PERM` | [Group permissions](PERM-group-permissions.md) | The policy set, evaluation, admin semantics, policy versioning | draft |
| `META` | [Group metadata and app data](META-group-metadata.md) | Immutable and mutable metadata, the app-data dictionary and registry, disappearing messages | draft |
| `DMS` | [DMs and stitching](DMS-direct-messages.md) | DM id, fixed DM policy, duplicate detection and stitching | draft |
| `PROC` | [Message processing](PROC-message-processing.md) | Streams and queries into one ordered processor, durable positions, retry and rejection | draft |
| `SEND` | [Message sending](SEND-message-sending.md) | Intents, message ids, publish retries, per-group ordering, failed sends | draft |
| `FORK` | [Fork recovery and commit log](FORK-fork-recovery.md) | Commit-log entries, fork detection, readd requests, one-shot groups | draft |
| `CONS` | [Consent](CONS-consent.md) | Consent states, precedence, defaults, gating of listing and streaming | draft |
| `SYNC` | [Device sync](SYNC-device-sync.md) | The sync group, preference updates, trust of sync invitations | draft |
| `ARCH` | [Archive format](ARCH-archive-format.md) | The backup export and import format and its compatibility promise | draft |
| `CTYPE` | [Content types](CTYPE-content-types.md) | Content type ids, the encoded-content envelope, the standard catalogue | draft |

## Tools

Run each one as `dev/nix-shell 'just <recipe>'`.

```bash
just spec-check            # validate docs/specs/ and the backlinks in code; part of just lint
just spec-index            # print every requirement with its links; --json for tooling
just spec-show JOIN-012    # print one requirement with its links
```

Skills for this work live in `.agents/skills/`: `authoring-specs`, `reviewing-specs`, and `checking-spec-compliance`.
