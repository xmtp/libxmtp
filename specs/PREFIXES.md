# Prefix registry

Every requirement identifier begins with a prefix registered here. The checker reads this table: a prefix that is not listed is an error, and a requirement identifier whose prefix is listed but whose number does not resolve to a current requirement is a stray mention (SPEC-053).

A prefix appears exactly once. A legacy prefix points at the legacy file it came from, so that stale mentions of its identifiers in code are still reported while that file is being retired.

## Active

| Prefix | Spec | File |
| --- | --- | --- |
| `SPEC` | Specification format | `specs/SPEC-spec-format.md` |
| `TOPIC` | Topic format | `specs/TOPIC-topic-format.md` |
| `API` | Backend API contract | `specs/API-backend-api.md` |
| `CONF` | Backend configuration | `specs/CONF-backend-configuration.md` |
| `AUTH` | Backend auth | `specs/AUTH-backend-auth.md` |
| `OPS` | Backend operations | `specs/OPS-backend-operations.md` |
| `PUSH` | Push subscriptions and webhooks | `specs/PUSH-push-subscriptions.md` |
| `IDENT` | Identity updates | `specs/IDENT-identity-updates.md` |
| `JOIN` | Joining groups | `specs/JOIN-joining-groups.md` |
| `GMOD` | Modifying groups | `specs/GMOD-modifying-groups.md` |
| `PERM` | Group permissions | `specs/PERM-group-permissions.md` |
| `META` | Group metadata and app data | `specs/META-group-metadata.md` |
| `DMS` | DMs and stitching | `specs/DMS-direct-messages.md` |
| `PROC` | Message processing | `specs/PROC-message-processing.md` |
| `SEND` | Message sending | `specs/SEND-message-sending.md` |
| `FORK` | Fork recovery and commit log | `specs/FORK-fork-recovery.md` |
| `CONS` | Consent | `specs/CONS-consent.md` |
| `SYNC` | Device sync | `specs/SYNC-device-sync.md` |
| `ARCH` | Archive format | `specs/ARCH-archive-format.md` |
| `CTYPE` | Content types | `specs/CTYPE-content-types.md` |

## Legacy

These prefixes belong to documents under `docs/specs/` that the specs above replace. They are listed so the checker reports stale references to them. A row is deleted with its file.

| Prefix | Legacy document | Replaced by |
| --- | --- | --- |
| `ARC` | `docs/specs/002_backend_architecture.md` | `API`, `OPS`, `AUTH` |
| `SEC` | `docs/specs/003_message_security.md` | `API`, `IDENT`, `JOIN` |
| `STR` | `docs/specs/004_streaming.md` | `API`, `PROC` |
| `CFG` | `docs/specs/006_server_configuration.md` | `CONF` |

## Reused prefixes and their floors

`API` and `PUSH` are reused by their replacements: the legacy documents `001_backend_api.md` and `005_push_subscriptions.md` used those prefixes, and the new specs take them over.

A reused prefix keeps its historical number space. An identifier that already meant something must never come to mean something else, or every past pull request, commit message, and review comment becomes ambiguous, and no cleanup of current code can repair that history. A replacement spec therefore allocates above the floor below, and reuses a historical number only when it carries the same obligation as before.

| Prefix | Highest legacy number | First number a replacement may allocate |
| --- | --- | --- |
| `API` | `API-165` | `API-200` |
| `PUSH` | `PUSH-105` | `PUSH-200` |

The floors are round numbers above the legacy maximum, so a late correction to a legacy document cannot collide with a new requirement.
