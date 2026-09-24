# Prefix registry

Every requirement identifier begins with a prefix registered here. The checker reads this table: a prefix that is not listed is an error, and a requirement identifier whose prefix is listed but whose number does not resolve to a current requirement is a stray mention (SPEC-053).

A prefix appears exactly once. A prefix whose document was deleted is removed from this file once no mention of its identifiers remains in the tree, so a stale mention is fixed rather than hidden.

## Active

| Prefix | Spec | File |
| --- | --- | --- |
| `SPEC` | Specification format | `docs/specs/SPEC-spec-format.md` |
| `TOPIC` | Topic format | `docs/specs/TOPIC-topic-format.md` |
| `API` | Backend API contract | `docs/specs/API-backend-api.md` |
| `CONF` | Backend configuration | `docs/specs/CONF-backend-configuration.md` |
| `AUTH` | Backend auth | `docs/specs/AUTH-backend-auth.md` |
| `OPS` | Backend operations | `docs/specs/OPS-backend-operations.md` |
| `PUSH` | Push subscriptions and webhooks | `docs/specs/PUSH-push-subscriptions.md` |
| `IDENT` | Identity updates | `docs/specs/IDENT-identity-updates.md` |
| `JOIN` | Joining groups | `docs/specs/JOIN-joining-groups.md` |
| `GMOD` | Modifying groups | `docs/specs/GMOD-modifying-groups.md` |
| `PERM` | Group permissions | `docs/specs/PERM-group-permissions.md` |
| `META` | Group metadata and app data | `docs/specs/META-group-metadata.md` |
| `DMS` | DMs and stitching | `docs/specs/DMS-direct-messages.md` |
| `PROC` | Message processing | `docs/specs/PROC-message-processing.md` |
| `SEND` | Message sending | `docs/specs/SEND-message-sending.md` |
| `FORK` | Fork recovery and commit log | `docs/specs/FORK-fork-recovery.md` |
| `CONS` | Consent | `docs/specs/CONS-consent.md` |
| `SYNC` | Device sync | `docs/specs/SYNC-device-sync.md` |
| `ARCH` | Archive format | `docs/specs/ARCH-archive-format.md` |
| `CTYPE` | Content types | `docs/specs/CTYPE-content-types.md` |
| `EVENT` | Client events | `docs/specs/EVENT-client-events.md` |

## Reused prefixes and their floors

`API` and `PUSH` are reused by their replacements: the deleted documents `001_backend_api.md` and `005_push_subscriptions.md` used those prefixes, and the new specs take them over.

A reused prefix keeps its historical number space. An identifier that already meant something must never come to mean something else, or every past pull request, commit message, and review comment becomes ambiguous, and no cleanup of current code can repair that history. A replacement spec therefore allocates above the floor below, and reuses a historical number only when it carries the same obligation as before.

| Prefix | Highest number the deleted document used | First number a replacement may allocate |
| --- | --- | --- |
| `API` | `API-165` | `API-200` |
| `PUSH` | `PUSH-105` | `PUSH-200` |

The floors are round numbers above the highest number those documents used, so no replacement requirement can take a number that already meant something else.
