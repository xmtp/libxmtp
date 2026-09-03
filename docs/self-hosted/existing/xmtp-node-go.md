<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# xmtp-node-go (XMTP v3 backend) — Behavior Reference

**Repository:** `/Users/nickmolnar/code/xmtp/xmtp-node-go`
**Last commit at time of writing:** `7561deb64aa5a324c13d68c8234bf7b4d7fcde9d` — *"fix(mls/api): prune and rotate XIP-83 catch-up wave scans onto per-topic ind..."*
**Language:** Go. **Database:** PostgreSQL (Bun migrations + sqlc queries).

This page records the CURRENT behavior of the v3 backend so a new Rust backend can match
client expectations. Every claim cites a file and a function. Where the code does not make
behavior clear, the text says "unverified".

---

## Table of contents

1. [Overview and process layout](#1-overview-and-process-layout)
2. [Scope note: the legacy v1/v2 message API](#2-scope-note-the-legacy-v1v2-message-api)
3. [Transport, interceptors, and gateway](#3-transport-interceptors-and-gateway)
4. [Global gates: version gating and the D14N cutover](#4-global-gates-version-gating-and-the-d14n-cutover)
5. [Database schema](#5-database-schema)
6. [Ordering model: how per-topic sequence ids are produced](#6-ordering-model-how-per-topic-sequence-ids-are-produced)
7. [MLS validation service (external gRPC dependency)](#7-mls-validation-service-external-grpc-dependency)
8. [MLS API endpoints](#8-mls-api-endpoints)
   - [8.1 SendGroupMessages](#81-sendgroupmessages)
   - [8.2 SendWelcomeMessages](#82-sendwelcomemessages)
   - [8.3 RegisterInstallation](#83-registerinstallation-deprecated)
   - [8.4 UploadKeyPackage](#84-uploadkeypackage)
   - [8.5 FetchKeyPackages](#85-fetchkeypackages)
   - [8.6 RevokeInstallation](#86-revokeinstallation)
   - [8.7 GetIdentityUpdates (MLS API, legacy)](#87-getidentityupdates-mls-api-legacy)
   - [8.8 QueryGroupMessages](#88-querygroupmessages)
   - [8.9 QueryWelcomeMessages](#89-querywelcomemessages)
   - [8.10 SubscribeGroupMessages](#810-subscribegroupmessages)
   - [8.11 SubscribeWelcomeMessages](#811-subscribewelcomemessages)
   - [8.12 BatchPublishCommitLog](#812-batchpublishcommitlog)
   - [8.13 BatchQueryCommitLog](#813-batchquerycommitlog)
   - [8.14 GetNewestGroupMessage](#814-getnewestgroupmessage)
9. [XIP-83 `Subscribe` bidirectional stream](#9-xip-83-subscribe-bidirectional-stream)
10. [Identity API endpoints](#10-identity-api-endpoints)
11. [D14N migration API](#11-d14n-migration-api)
12. [Live delivery: DB poller and subscription dispatcher](#12-live-delivery-db-poller-and-subscription-dispatcher)
13. [Rate limiting](#13-rate-limiting)
14. [Authorization (IP allow list)](#14-authorization-ip-allow-list)
15. [Backfillers](#15-backfillers)
16. [Pruning / expiry](#16-pruning--expiry)
17. [Server configuration options](#17-server-configuration-options)
18. [Metrics](#18-metrics)
19. [Summary of limits and constants](#19-summary-of-limits-and-constants)
20. [Open questions / unverified items](#20-open-questions--unverified-items)

---

## 1. Overview and process layout

The binary is `cmd/xmtpd`. `pkg/server/server.go` builds a process that contains:

- A libp2p / go-waku node used only by the legacy v1/v2 message API.
- A gRPC server on `--api.grpc-port` (default 5556), see `pkg/api/server.go:startGRPC`.
- A grpc-gateway HTTP server on `--api.http-port` (default 5555), see
  `pkg/api/server.go:startHTTP`.
- Three gRPC services relevant to v3:
  - `xmtp.mls.api.v1.MlsApi` — `pkg/mls/api/v1/service.go` and `pkg/mls/api/v1/subscribe.go`.
  - `xmtp.identity.api.v1.IdentityApi` — `pkg/identity/api/v1/identity_service.go`.
  - `xmtp.migration.api.v1.D14nMigrationApi` — `pkg/migration/api/v1/service.go`. Note the
    **lowercase `n`** in the wire service name; the generated Go identifiers spell it
    `D14NMigrationApi` (§11).
- A legacy `xmtp.message_api.v1.MessageApi` service — `pkg/api/message/v1/service.go`.

MLS and Identity are registered only when a store and a validator are configured and
`--api.enable-mls` is set (`pkg/api/server.go:startGRPC`).

Two store handles exist: a read-write store bound to the primary database and a read-only
store that may be bound to a read replica (`pkg/api/config.go:Config` fields `MLSStore` and
`ReadMLSStore`; `pkg/mls/store/store.go:New`, `pkg/mls/store/readStore.go:NewReadStore`).

---

## 2. Scope note: the legacy v1/v2 message API

`xmtp.message_api.v1.MessageApi` (`pkg/api/message/v1/service.go`) still exists in the
process. It implements `Publish`, `Subscribe`, `Subscribe2`, `SubscribeAll`, `Query` and
`BatchQuery` over waku relay plus a `pkg/store` Postgres message table
(`pkg/migrations/messages`). It carries XMTP v1 and v2 traffic (invites, contacts,
conversations, user preferences), not MLS. It is explicitly out of scope for the new
backend and is not documented further here, except that it shares the rate limiter
(`pkg/api/interceptor.go:applyLimits` handles `*messagev1.PublishRequest` and
`*messagev1.BatchQueryRequest`) and the same subscription dispatcher instance
(`pkg/api/server.go:startGRPC` creates one `subscriptions.NewSubscriptionDispatcher` and
passes it to both the message service and the MLS service).

---

## 3. Transport, interceptors, and gateway

### gRPC server

Set up in `pkg/api/server.go:startGRPC`.

- The listener is wrapped in `proxyproto.Listener` with `ReadHeaderTimeout: 10 * time.Second`,
  so the node accepts the PROXY protocol from a load balancer.
- Server options:
  - `grpc.Creds(insecure.NewCredentials())` — TLS is terminated upstream.
  - `grpc.KeepaliveParams(keepalive.ServerParameters{Time: 5 * time.Minute})`.
  - `grpc.KeepaliveEnforcementPolicy(keepalive.EnforcementPolicy{PermitWithoutStream: true,
    MinTime: 15 * time.Second})`.
  - `grpc.MaxRecvMsgSize(s.MaxMsgSize)` — default 52428800 bytes (50 MiB), from
    `pkg/api/config.go:Options.MaxMsgSize`. There is no explicit `MaxSendMsgSize`, so the
    gRPC default (math.MaxInt32) applies on the send side.
- gzip is registered as a compressor via the blank import
  `_ "google.golang.org/grpc/encoding/gzip"`.
- `healthgrpc.RegisterHealthServer` registers `grpc.health.v1.Health`.
- `reflection.Register(grpcServer)` enables server reflection.

Interceptor chain order (`pkg/api/server.go:startGRPC`), both unary and stream:

1. `prometheus.UnaryServerInterceptor` / `prometheus.StreamServerInterceptor`
   (`go-grpc-prometheus`, with `EnableHandlingTimeHistogram()` applied once).
2. `TelemetryInterceptor` (`pkg/api/telemetry.go`).
3. `GatingInterceptor` (`pkg/api/gating.go`) — libxmtp version gate, see §4.
4. `RateLimitInterceptor` (`pkg/api/interceptor.go`) — only when `--api.authn.ratelimits`
   is set.

### HTTP gateway

`pkg/api/server.go:startHTTP` builds a `runtime.NewServeMux` with:

- `runtime.WithMarshalerOption("application/x-protobuf", &runtime.ProtoMarshaller{})` — a
  client may send/receive binary protobuf over HTTP by setting that content type.
- `runtime.WithErrorHandler(runtime.DefaultHTTPErrorHandler)` and
  `runtime.WithStreamErrorHandler(runtime.DefaultStreamErrorHandler)`.
- `runtime.WithIncomingHeaderMatcher(incomingHeaderMatcher)` — only `x-app-version`,
  `x-client-version` and `x-libxmtp-version` are forwarded into gRPC metadata
  (`pkg/api/server.go:incomingHeaderMatcher`, keys in
  `pkg/api/message/v1/context/context.go`).
- Path `/` and `/swagger-ui*` serve a Swagger UI; `/swagger.json` serves
  `pkg/proto/openapi` JSON; everything else falls through to the gateway mux.
- The whole handler is wrapped by `gzipWrapper` then `allowCORS`
  (`pkg/api/server.go:allowCORS`, `pkg/api/gzip_handler.go`). CORS sets
  `Access-Control-Allow-Origin: *` and, on preflight, allows methods
  `GET, HEAD, POST, PUT, PATCH, DELETE` and the header list in
  `pkg/api/server.go:preflightHandler` (`Content-Type, Accept, Authorization, X-App-Version,
  X-Client-Version, X-Libxmtp-Version, Baggage, DNT, Sec-CH-UA, Sec-CH-UA-Mobile,
  Sec-CH-UA-Platform, Sentry-Trace, User-Agent, x-libxmtp-version, x-app-version`).
- The gateway dials its own gRPC listener over `passthrough://localhost/<addr>` with
  `grpc.MaxCallRecvMsgSize(s.MaxMsgSize)` (`pkg/api/server.go:dialGRPC`).

### HTTP routes

From `pkg/proto/mls/api/v1/mls.pb.gw.go` (all POST):

| Method | HTTP path |
| --- | --- |
| SendGroupMessages | `/mls/v1/send-group-messages` |
| SendWelcomeMessages | `/mls/v1/send-welcome-messages` |
| RegisterInstallation | `/mls/v1/register-installation` |
| UploadKeyPackage | `/mls/v1/upload-key-package` |
| FetchKeyPackages | `/mls/v1/fetch-key-packages` |
| RevokeInstallation | `/mls/v1/revoke-installation` |
| GetIdentityUpdates | `/mls/v1/get-identity-updates` |
| QueryGroupMessages | `/mls/v1/query-group-messages` |
| QueryWelcomeMessages | `/mls/v1/query-welcome-messages` |
| SubscribeGroupMessages | `/mls/v1/subscribe-group-messages` |
| SubscribeWelcomeMessages | `/mls/v1/subscribe-welcome-messages` |
| BatchPublishCommitLog | `/mls/v1/batch-publish-commit-log` |
| BatchQueryCommitLog | `/mls/v1/batch-query-commit-log` |
| GetNewestGroupMessage | `/mls/v1/get-newest-group-message` |

**`Subscribe` (XIP-83) has no HTTP gateway route.** `pkg/proto/mls/api/v1/mls.pb.gw.go`
contains no pattern for it, because grpc-gateway cannot express a bidirectional stream.
It is gRPC-only.

From `pkg/proto/identity/api/v1/identity.pb.gw.go`:

| Method | HTTP path |
| --- | --- |
| PublishIdentityUpdate | `/identity/v1/publish-identity-update` |
| GetIdentityUpdates | `/identity/v1/get-identity-updates` |
| GetInboxIds | `/identity/v1/get-inbox-ids` |
| VerifySmartContractWalletSignatures | `/identity/v1/verify-smart-contract-wallet-signatures` |

gRPC full method names are in `pkg/proto/mls/api/v1/mls_grpc.pb.go`, for example
`/xmtp.mls.api.v1.MlsApi/SendGroupMessages` and `/xmtp.mls.api.v1.MlsApi/Subscribe`.

---

## 4. Global gates: version gating and the D14N cutover

### libxmtp version gating

`pkg/api/gating.go:GatingInterceptor` runs on every unary and stream call. It builds a
`requesterInfo` from gRPC metadata (`pkg/api/message/v1/context/context.go:NewRequesterInfo`)
and rejects unsupported clients.

`pkg/api/message/v1/context/context.go:isSupportedClient`:

- If the process env var `ENV` is not `production`, every client is supported.
- Otherwise the **complete `x-libxmtp-version` header value** is used, then normalized by
  prefixing `v` if missing. `parseVersionHeaderValue` returns three values
  (`name`, `version`, `full`) and `NewRequesterInfo` assigns the **third** (`full`) to
  `ri.LibxmtpVersion` — so the `name/version` split is computed and then discarded for the
  libxmtp header. The `x-app-version` and `x-client-version` headers do use the split parts.
- If the value is not valid semver, the client is treated as supported (fail open).
- Otherwise the client is supported when `semver.Compare(fullHeaderValue, "v1.1.5") >= 0`.

**Consequence:** a header in the conventional `name/version` form, for example
`libxmtp/1.1.4`, normalizes to `vlibxmtp/1.1.4`, which is **not valid semver**, so it fails
open and is allowed. The gate only rejects a header whose *entire* value is valid semver
below `v1.1.5` — for example a bare `1.1.4` or `v1.1.4`. A new backend should not assume the
gate parses a `name/version` pair.

| Condition | gRPC code | Message |
| --- | --- | --- |
| `ENV=production` and the whole `x-libxmtp-version` header value is valid semver `< v1.1.5` | `Unimplemented` | `unsupported libxmtp version <full header value>` |

The constant `IS_GATING_ENABLED = true` in `pkg/api/gating.go` is compiled in; it is not a
flag.

### D14N cutover

`pkg/migration/cutover.go:CutoverChecker.IsMigrationComplete` returns true when
`cutoverNs != 0` and `time.Now().UnixNano() >= cutoverNs`. The checker is created only when
`--api.enable-migration` is set, and `--api.d14n-cutover-ns` must then be non-zero
(`pkg/api/server.go:startGRPC` returns `d14n-cutover-ns must be specified when migration is
enabled` otherwise).

`pkg/mls/api/v1/service.go:isPublishDisabled` returns true when the process flag
`--api.disable-mls-publish` is set OR the cutover has passed.
`pkg/mls/api/v1/service.go:isStreamingDisabled` returns true only when the cutover has
passed.

Effects:

| Surface | Guard | Code | Message |
| --- | --- | --- | --- |
| RegisterInstallation, UploadKeyPackage, SendGroupMessages, SendWelcomeMessages, BatchPublishCommitLog | `isPublishDisabled()` | `Unavailable` | `publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N.` |
| PublishIdentityUpdate | `isPublishDisabled()` (`pkg/identity/api/v1/identity_service.go`) | `Unavailable` | same text |
| SubscribeGroupMessages, SubscribeWelcomeMessages | `isStreamingDisabled()` at start, and re-checked every 5 s on an in-stream ticker | `Unavailable` | `XMTP V3 streaming is no longer available. Please upgrade your client to XMTP D14N.` |

Note: **the XIP-83 `Subscribe` handler does not check the cutover.**
`pkg/mls/api/v1/subscribe.go:Subscribe` has no `isStreamingDisabled` call. This differs from
the two legacy subscribe methods.

Query endpoints (`QueryGroupMessages`, `QueryWelcomeMessages`, `BatchQueryCommitLog`,
`GetNewestGroupMessage`, `FetchKeyPackages`) and the read-only Identity endpoints are never
gated by the cutover.

---

## 5. Database schema

Sources: `pkg/migrations/mls/*.sql` (Bun migrations, embedded via
`pkg/migrations/mls/migrations.go`), `pkg/mls/store/queries/models.go` (sqlc models), and
`pkg/migrations/authz/*.sql`.

Bun splits a migration file on the `--bun:split` marker. `sqlc.yaml` points sqlc at
`pkg/mls/store/queries.sql` for queries and at the **migration directory itself**
(`pkg/migrations/mls`) for the schema; there is no standalone `schema.sql`.

### 5.1 Migration history (`pkg/migrations/mls`)

| # | File | Effect |
| --- | --- | --- |
| 1 | `20240528181822_wipe-db` | `DROP TABLE IF EXISTS installations, group_messages, welcome_messages, inbox_log, address_log; DROP TYPE IF EXISTS inbox_filter;` — clean slate. Down is a no-op. |
| 2 | `20240528181851_init-schema` | Creates `installations`, `group_messages`, `welcome_messages`, `inbox_log`, `address_log`, their indexes, and the `inbox_filter` composite type. Down is a no-op. |
| 3 | `20240814032323_remove-inbox-id-from-installation` | `ALTER TABLE installations DROP COLUMN inbox_id, DROP COLUMN expiration`. |
| 4 | `20240829001344_serial-ids` | Creates `insert_group_message`, `insert_welcome_message`, `insert_inbox_log` — the advisory-lock insert functions (§6). No table DDL. |
| 5 | `20250313220142_add-inboxes-table` | `CREATE TABLE IF NOT EXISTS inboxes(id BYTEA PRIMARY KEY, updated_at TIMESTAMP DEFAULT NOW())`. |
| 6 | `20250409031523_add-welcome-encryption` | `ALTER TABLE welcome_messages ADD COLUMN wrapper_algorithm SMALLINT NOT NULL DEFAULT 0`; creates `insert_welcome_message_v2`. |
| 7 | `20250604154813_add_refresh_cursor_to_welcomes` | Adds `welcome_messages.message_cursor BIGINT NOT NULL DEFAULT 0`; creates `insert_welcome_message_v3`. **The column is removed again by migration 9 and is not in the final schema.** |
| 8 | `20250621013154_add_commit_log` | Creates `commit_log(id BIGSERIAL PK, created_at TIMESTAMP NOT NULL DEFAULT NOW(), group_id BYTEA NOT NULL, encrypted_entry BYTEA NOT NULL)` and `insert_commit_log`. No index beyond the PK. |
| 9 | `20250625195628_add_welcome_metadata` | `DROP COLUMN message_cursor`; `ADD COLUMN welcome_metadata BYTEA` (nullable); creates `insert_welcome_message_v4`. |
| 10 | `20250707105710_add_is_commit` | `ALTER TABLE group_messages ADD COLUMN is_commit BOOLEAN DEFAULT NULL`; creates `insert_group_message_with_is_commit`. |
| 11 | `20250709203710_add_backfill_idx` | `CREATE INDEX idx_group_messages_is_commit_null_id_asc ON group_messages (id) WHERE is_commit IS NULL` — partial index for the backfiller. |
| 12 | `20250710145538_add-sender-hmac` | Adds `group_messages.sender_hmac BYTEA` and `group_messages.should_push BOOLEAN` (both nullable, no default); creates `insert_group_message_v3` — the live group insert path. |
| 13 | `20250714160210_add_key_packages_migration` | Adds `installations.is_appended BOOLEAN DEFAULT NULL`; creates `key_packages` with `UNIQUE (installation_id, key_package)` and `idx_key_packages_installation_id`. |
| 14 | `20250730004640_modify_commit_log` | Creates `commit_log_v2` + `idx_commit_log_v2_group_id_id` + `insert_commit_log_v2`. Carries `TODO(rich): Drop the old commit log table and function once servers have been deployed`. |
| 15 | `20250926170348_add_welcome_pointer_support` | Adds `welcome_messages.message_type SMALLINT NOT NULL DEFAULT 0`; drops NOT NULL on `hpke_public_key`; creates `insert_welcome_pointer_message_v1`. |

There is no `insert_group_message_v2`: the group insert lineage is
`insert_group_message` → `insert_group_message_with_is_commit` → `insert_group_message_v3`.

### 5.2 Final consolidated schema

#### `installations`

Stores one row per MLS installation, holding its current ("last resort") key package.

| Column | Type | Null | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | BYTEA | NOT NULL | — | PRIMARY KEY. The installation public key derived by the validation service. |
| `created_at` | BIGINT | NOT NULL | — | **Epoch nanoseconds**, written by Go (`nowNs()`), not a timestamp column. |
| `updated_at` | BIGINT | NOT NULL | — | Epoch nanoseconds. |
| `key_package` | BYTEA | NOT NULL | — | Latest key package, overwritten on upsert. |
| `is_appended` | BOOLEAN | NULL | NULL | Tri-state backfill marker (NULL = not yet copied into `key_packages`). |

Dropped over time: `inbox_id BYTEA NOT NULL`, `expiration BIGINT NOT NULL`.
Indexes: `installations_pkey` on `(id)` only.

#### `group_messages`

The MLS group message log. One row per published application or commit message.

| Column | Type | Null | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | BIGSERIAL (bigint) | NOT NULL | `nextval('group_messages_id_seq')` | PRIMARY KEY. **This is the cursor / sequence id clients see.** |
| `created_at` | TIMESTAMP (no tz) | NOT NULL | `NOW()` | Server clock; exposed as `created_ns`. |
| `group_id` | BYTEA | NOT NULL | — | Topic key. Comes from the validation service. |
| `data` | BYTEA | NOT NULL | — | TLS-serialized MLS message, opaque to the node. |
| `group_id_data_hash` | BYTEA | NOT NULL | — | `sha256(group_id ‖ data)`; the dedup key. |
| `is_commit` | BOOLEAN | NULL | NULL | NULL = not yet classified by the backfiller. |
| `sender_hmac` | BYTEA | NULL | — | Opaque push-routing hint. |
| `should_push` | BOOLEAN | NULL | — | Opaque push flag. |

Indexes:

- `group_messages_pkey` UNIQUE on `(id)`
- `idx_group_messages_group_id_id` on `(group_id, id)` — the index every per-topic query and
  the XIP-83 LATERAL wave scan rides.
- `idx_group_messages_group_id_data_hash` UNIQUE on `(group_id_data_hash)` — the idempotency
  constraint.
- `idx_group_messages_is_commit_null_id_asc` on `(id)` **WHERE `is_commit IS NULL`** (partial).

#### `welcome_messages`

The MLS welcome log, one row per welcome or welcome pointer, keyed by recipient installation.

| Column | Type | Null | Default | Notes |
| --- | --- | --- | --- | --- |
| `id` | BIGSERIAL (bigint) | NOT NULL | `nextval('welcome_messages_id_seq')` | PRIMARY KEY, the client cursor. |
| `created_at` | TIMESTAMP | NOT NULL | `NOW()` | |
| `installation_key` | BYTEA | NOT NULL | — | Topic key (recipient installation). |
| `data` | BYTEA | NOT NULL | — | Welcome payload, **or** the welcome-pointer payload when `message_type = 1`. |
| `hpke_public_key` | BYTEA | **NULL** | — | Was NOT NULL until migration 15. |
| `installation_key_data_hash` | BYTEA | NOT NULL | — | `sha256(installation_key ‖ data)`; the dedup key. |
| `wrapper_algorithm` | SMALLINT | NOT NULL | `0` | **Go enum ordering, not proto values** — see §8.2. |
| `welcome_metadata` | BYTEA | NULL | — | Opaque. |
| `message_type` | SMALLINT | NOT NULL | `0` | 0 = welcome, 1 = welcome pointer. Unknown values are skipped by every reader. |

Removed: `message_cursor BIGINT NOT NULL DEFAULT 0` (added migration 7, dropped migration 9).

Indexes:

- `welcome_messages_pkey` UNIQUE on `(id)`
- `idx_welcome_messages_installation_key_id` on `(installation_key, id)`
- `idx_welcome_messages_group_key_data_hash` UNIQUE on `(installation_key_data_hash)` — the
  name says "group_key" but the column is the installation-key hash.

#### `inbox_log`

The append-only identity update log, one row per published `IdentityUpdate`.

| Column | Type | Null | Default | Notes |
| --- | --- | --- | --- | --- |
| `sequence_id` | BIGSERIAL (bigint) | NOT NULL | `nextval('inbox_log_sequence_id_seq')` | PRIMARY KEY, returned to clients as `sequence_id`. |
| `inbox_id` | BYTEA | NOT NULL | — | Raw bytes; the API takes it as hex and uses `decode(...,'hex')`. |
| `server_timestamp_ns` | BIGINT | NOT NULL | — | Written by Go (`nowNs()`), not the database. |
| `identity_update_proto` | BYTEA | NOT NULL | — | Marshalled `associations.IdentityUpdate`. |

Indexes: `inbox_log_pkey` on `(sequence_id)`; `idx_inbox_log_inbox_id_sequence_id` on
`(inbox_id, sequence_id)`.

Capped at **256 entries per inbox** by application logic
(`pkg/mls/store/store.go:PublishIdentityUpdate`), not by a constraint.

#### `address_log`

Maps an Ethereum address to the inbox that currently owns it. Append-only, plus in-place
revocation updates.

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `address` | TEXT | NOT NULL | — |
| `inbox_id` | BYTEA | NOT NULL | — |
| `association_sequence_id` | BIGINT | NULL | — |
| `revocation_sequence_id` | BIGINT | NULL | — |

**No primary key and no unique constraint.** Only `idx_address_log_address_inbox_id` on
`(address, inbox_id)`. The sequence-id columns reference `inbox_log.sequence_id` values but
there is no foreign key. "Current" association = highest `association_sequence_id` for the
address with `revocation_sequence_id IS NULL`.

#### `inboxes`

An activity-timestamp table, written by `TouchInbox` on every identity update.

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `id` | BYTEA | NOT NULL | — PRIMARY KEY |
| `updated_at` | TIMESTAMP | NULL | `NOW()` |

Nothing reads it in the API path.

#### `key_packages`

Accumulates every distinct key package an installation has ever uploaded.

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `sequence_id` | BIGSERIAL (bigint) | NOT NULL | `nextval('key_packages_sequence_id_seq')`, PRIMARY KEY |
| `installation_id` | BYTEA | NOT NULL | — |
| `key_package` | BYTEA | NOT NULL | — |
| `created_at` | BIGINT | NOT NULL | — (epoch **nanoseconds**) |

Constraints/indexes: `key_packages_pkey`; `UNIQUE (installation_id, key_package)` (implicit
index `key_packages_installation_id_key_package_key`, the `ON CONFLICT` target);
`idx_key_packages_installation_id`.

**`FetchKeyPackages` does not read this table** — it reads `installations`.

#### `commit_log` (v1, deprecated, still present)

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `id` | BIGSERIAL | NOT NULL | `nextval('commit_log_id_seq')`, PK |
| `created_at` | TIMESTAMP | NOT NULL | `NOW()` |
| `group_id` | BYTEA | NOT NULL | — |
| `encrypted_entry` | BYTEA | NOT NULL | — |

PK index only. No query in `queries.sql` reads or writes it; only the `CommitLog` Go model
remains.

#### `commit_log_v2`

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `id` | BIGSERIAL | NOT NULL | `nextval('commit_log_v2_id_seq')`, PK — exposed as `sequence_id` in the API |
| `created_at` | TIMESTAMP | NOT NULL | `NOW()` |
| `group_id` | BYTEA | NOT NULL | — |
| `serialized_entry` | BYTEA | NOT NULL | — |
| `serialized_signature` | BYTEA | NOT NULL | — |

Indexes: `commit_log_v2_pkey` on `(id)`; `idx_commit_log_v2_group_id_id` on `(group_id, id)`.
**No unique constraint** — the same entry published twice yields two rows.

#### Composite type `inbox_filter`

```sql
CREATE TYPE inbox_filter AS (
    inbox_id TEXT,   -- TEXT because it is serialized as JSON
    sequence_id BIGINT
);
```

Used only as the target of `json_populate_recordset` in `GetInboxLogFiltered`, so a batched
`GetIdentityUpdates` is one round trip. The JSON keys come from
`pkg/mls/store/queries/filters.go:InboxLogFilter` (`json:"inbox_id"`, `json:"sequence_id"`)
and must match the type's attribute names.

### 5.3 Sequences

All are implicit, created by `BIGSERIAL`/`SERIAL`. There is no explicit `CREATE SEQUENCE`.

| Sequence | Column |
| --- | --- |
| `group_messages_id_seq` | `group_messages.id` |
| `welcome_messages_id_seq` | `welcome_messages.id` |
| `inbox_log_sequence_id_seq` | `inbox_log.sequence_id` |
| `commit_log_id_seq` | `commit_log.id` |
| `commit_log_v2_id_seq` | `commit_log_v2.id` |
| `key_packages_sequence_id_seq` | `key_packages.sequence_id` |
| `authz_addresses_id_seq` | `authz_addresses.id` (int4) |
| `ip_addresses_id_seq` | `ip_addresses.id` (int4) |

`installations` and `inboxes` have BYTEA primary keys and no sequence.

### 5.4 Every `ON CONFLICT` clause

There are exactly three:

1. `CreateOrUpdateInstallation` — `ON CONFLICT (id) DO UPDATE SET key_package = ..., updated_at = ...`.
   `created_at` and `is_appended` survive.
2. `TouchInbox` — `ON CONFLICT (id) DO UPDATE SET updated_at = NOW()`.
3. `InsertKeyPackage` — `ON CONFLICT (installation_id, key_package) DO NOTHING`.

`group_messages` and `welcome_messages` have **no** `ON CONFLICT`: their duplicate
suppression is a raised unique-violation on the hash index, caught in Go by string-matching
`"duplicate key value violates unique constraint"`
(`pkg/mls/store/store.go:InsertGroupMessage`). `address_log` inserts are unguarded.

### 5.5 sqlc models (`pkg/mls/store/queries/models.go`)

sqlc v1.30.0, default snake_case → CamelCase with `id`→`ID`. Note `sender_hmac` →
`SenderHmac` (not `SenderHMAC`) and `server_timestamp_ns` → `ServerTimestampNs`.

| Struct | Field | Go type | Column |
| --- | --- | --- | --- |
| `AddressLog` | `Address` | `string` | `address` |
| | `InboxID` | `[]byte` | `inbox_id` |
| | `AssociationSequenceID` | `sql.NullInt64` | `association_sequence_id` |
| | `RevocationSequenceID` | `sql.NullInt64` | `revocation_sequence_id` |
| `CommitLog` | `ID` | `int64` | `id` |
| | `CreatedAt` | `time.Time` | `created_at` |
| | `GroupID` | `[]byte` | `group_id` |
| | `EncryptedEntry` | `[]byte` | `encrypted_entry` |
| `CommitLogV2` | `ID` | `int64` | `id` |
| | `CreatedAt` | `time.Time` | `created_at` |
| | `GroupID` | `[]byte` | `group_id` |
| | `SerializedEntry` | `[]byte` | `serialized_entry` |
| | `SerializedSignature` | `[]byte` | `serialized_signature` |
| `GroupMessage` | `ID` | `int64` | `id` |
| | `CreatedAt` | `time.Time` | `created_at` |
| | `GroupID` | `[]byte` | `group_id` |
| | `Data` | `[]byte` | `data` |
| | `GroupIDDataHash` | `[]byte` | `group_id_data_hash` |
| | `IsCommit` | `sql.NullBool` | `is_commit` |
| | `SenderHmac` | `[]byte` | `sender_hmac` |
| | `ShouldPush` | `sql.NullBool` | `should_push` |
| `Inbox` | `ID` | `[]byte` | `id` |
| | `UpdatedAt` | `sql.NullTime` | `updated_at` |
| `InboxLog` | `SequenceID` | `int64` | `sequence_id` |
| | `InboxID` | `[]byte` | `inbox_id` |
| | `ServerTimestampNs` | `int64` | `server_timestamp_ns` |
| | `IdentityUpdateProto` | `[]byte` | `identity_update_proto` |
| `Installation` | `ID` | `[]byte` | `id` |
| | `CreatedAt` | `int64` | `created_at` |
| | `UpdatedAt` | `int64` | `updated_at` |
| | `KeyPackage` | `[]byte` | `key_package` |
| | `IsAppended` | `sql.NullBool` | `is_appended` |
| `KeyPackage` | `SequenceID` | `int64` | `sequence_id` |
| | `InstallationID` | `[]byte` | `installation_id` |
| | `KeyPackage` | `[]byte` | `key_package` |
| | `CreatedAt` | `int64` | `created_at` |
| `WelcomeMessage` | `ID` | `int64` | `id` |
| | `CreatedAt` | `time.Time` | `created_at` |
| | `InstallationKey` | `[]byte` | `installation_key` |
| | `Data` | `[]byte` | `data` |
| | `HpkePublicKey` | `[]byte` | `hpke_public_key` |
| | `InstallationKeyDataHash` | `[]byte` | `installation_key_data_hash` |
| | `WrapperAlgorithm` | `int16` | `wrapper_algorithm` |
| | `WelcomeMetadata` | `[]byte` | `welcome_metadata` |
| | `MessageType` | `int16` | `message_type` |

There is no sqlc model for `authz_addresses`, `ip_addresses` or `message` — those are managed
by the Bun ORM.

### 5.6 Every named sqlc query

Source `pkg/mls/store/queries.sql`, generated into
`pkg/mls/store/queries/queries.sql.go`. **43 named queries** (`grep -c '^-- name:'`), all of
which are listed below.

| Query | Kind | What it does |
| --- | --- | --- |
| `LockInboxLog` | exec | `SELECT pg_advisory_xact_lock(hashtext(@inbox_id))` — serializes identity publishing per inbox. Single-argument lock form; a **different lock space** from `insert_inbox_log`'s two-key lock. |
| `GetAllInboxLogs` | many | All `inbox_log` rows for one inbox, `ORDER BY sequence_id ASC`. Returns `inbox_id` re-encoded as hex. |
| `GetInboxLogFiltered` | many | Batch cursor read: joins `inbox_log` against a JSON array of `(inbox_id, sequence_id)` filters via `json_populate_recordset(NULL::inbox_filter, @filters)`, `a.sequence_id > b.sequence_id`, `ORDER BY a.sequence_id ASC`. |
| `GetAddressLogs` | many | For each address in `@addresses::TEXT[]`, the row with `MAX(association_sequence_id)` among those with `revocation_sequence_id IS NULL`. |
| `InsertAddressLog` | one | Appends an association row. No `ON CONFLICT` (no unique constraint exists). |
| `InsertInboxLog` | one | `SELECT sequence_id FROM insert_inbox_log(decode(@inbox_id,'hex'), @server_timestamp_ns, @identity_update_proto)` — returns the new sequence id. |
| `RevokeAddressFromLog` | exec | Sets `revocation_sequence_id` on the row with the max `association_sequence_id` for that (address, inbox). |
| `CreateOrUpdateInstallation` | exec | Upsert on `installations.id`; updates `key_package` and `updated_at`. |
| `GetInstallation` | one | One installation by id (excludes `is_appended`). |
| `FetchKeyPackages` | many | `SELECT id, key_package FROM installations WHERE id = ANY(@installation_ids::BYTEA[])`. |
| `InsertGroupMessage` | one | `SELECT * FROM insert_group_message_v3(...)` — the live group insert. |
| `InsertWelcomeMessage` | one | `SELECT * FROM insert_welcome_message_v4(...)` — regular welcome, `message_type` defaults to 0. |
| `InsertWelcomePointerMessage` | one | `insert_welcome_pointer_message_v1(...)` — forces `message_type = 1`, columns listed explicitly. |
| `GetAllGroupMessages` | many | Full table scan ascending. Test/tooling only. |
| `QueryGroupMessages` | many | First page for a group; direction chosen at runtime with the `CASE WHEN @sort_desc` trick so it stays one prepared statement. |
| `QueryGroupMessagesWithCursorAsc` | many | `group_id = @group_id AND id > @cursor ORDER BY id ASC LIMIT @numrows`. |
| `QueryGroupMessagesWithCursorDesc` | many | `... AND id < @cursor ORDER BY id DESC LIMIT @numrows`. |
| `GetAllWelcomeMessages` | many | Full scan ascending. |
| `QueryWelcomeMessages` | many | First page for an installation, runtime direction. |
| `QueryWelcomeMessagesWithCursorAsc` | many | `installation_key = ... AND id > @cursor ASC`. |
| `QueryWelcomeMessagesWithCursorDesc` | many | `... AND id < @cursor DESC`. |
| `QueryCommitLogV2` | many | `group_id = @group_id AND id > @cursor ORDER BY id ASC LIMIT @numrows`. Ascending only. |
| `TouchInbox` | exec | Upsert `inboxes`, refresh `updated_at`. |
| `GetOldWelcomeMessages` | one | Count welcomes older than `@age_days` (timestamp comparison). |
| `DeleteOldWelcomeMessagesBatch` | many | CTE `LIMIT @batch_size FOR UPDATE SKIP LOCKED` then delete, returning `(id, created_at)`. |
| `InsertCommitLogV2` | one | `SELECT * FROM insert_commit_log_v2(...)`. |
| `CountDeletableGroupMessages` | one | `WHERE is_commit = FALSE AND created_at < NOW() - make_interval(days := @age_days)`. NULL `is_commit` is excluded. |
| `DeleteOldGroupMessagesBatch` | many | Same batched pattern, also filtered on `is_commit = FALSE`. |
| `GetOldInstallations` | one | Count installations older than `@age_days`. **Compares a BIGINT-nanoseconds column to a timestamp expression — almost certainly wrong**, unlike the matching delete query. |
| `DeleteOldInstallationsBatch` | many | Batched delete; converts the cutoff to epoch nanoseconds: `created_at < (EXTRACT(EPOCH FROM NOW() - (@age_days::INT \|\| ' days')::INTERVAL) * 1e9)::BIGINT`. |
| `SelectEnvelopesForIsCommitBackfill` | many | `WHERE is_commit IS NULL ORDER BY id ASC FOR UPDATE SKIP LOCKED LIMIT 100` (hardcoded 100). Uses the partial index. |
| `UpdateIsCommitStatus` | exec | Writes the backfilled classification for one id. |
| `GetAllGroupMessagesWithCursor` | many | Global cross-topic forward scan `id > @cursor ORDER BY id ASC LIMIT @numrows` — the live poller's query. |
| `GetAllWelcomeMessagesWithCursor` | many | Same for welcomes. |
| `GetLatestGroupMessageID` | one | `COALESCE((SELECT max(id) FROM group_messages), 0)::BIGINT` — poller start point and XIP-83 wave ceiling. |
| `GetLatestWelcomeMessageID` | one | Same for welcomes. |
| `SelectInstallationsToBackfill` | many | `WHERE is_appended IS NULL ... FOR UPDATE SKIP LOCKED LIMIT 100`. No supporting partial index. |
| `UpdateIsAppendedStatus` | exec | Marks one installation backfilled. |
| `InsertKeyPackage` | exec | Idempotent append with `ON CONFLICT (installation_id, key_package) DO NOTHING`. |
| `GetOldKeyPackages` | one | Count key packages older than `@age_days`. Same BIGINT-ns/timestamp mismatch as `GetOldInstallations`. |
| `DeleteOldKeyPackagesBatch` | many | Batched delete. **The CTE selects `installation_id`, not the PK**, so the DELETE removes every row for each matched installation, including rows newer than the cutoff; `batch_size` bounds installations, not rows. |
| `GetNewestGroupMessage` | many | `SELECT DISTINCT ON (group_id) id, group_id, data, created_at, should_push, sender_hmac, is_commit ... ORDER BY group_id, id DESC`. |
| `GetNewestGroupMessageMetadata` | many | Same without `data`, `should_push`, `sender_hmac`. |

Two raw (non-sqlc) queries live in `pkg/mls/store/readStore.go` as string constants:
`queryGroupMessagesWaveScan` and `queryWelcomeMessagesWaveScan` (§9.11).

### 5.7 authz schema (`pkg/migrations/authz`)

`ip_addresses` — the live table (migration `20250709043343_add-ip-address-table`):

| Column | Type | Null | Default |
| --- | --- | --- | --- |
| `id` | SERIAL | NOT NULL | PRIMARY KEY |
| `created_at` | TIMESTAMPTZ | NULL | `NOW()` |
| `deleted_at` | TIMESTAMPTZ | NULL | — (soft delete) |
| `ip_address` | TEXT | NOT NULL | — |
| `permission` | TEXT | NOT NULL | — |
| `comment` | TEXT | NULL | — |

Index: `unique_ip_address` UNIQUE on `(ip_address)` **WHERE `deleted_at IS NULL`**, so many
soft-deleted rows can coexist per address.

The migration's SQL comment says the permission values are `'allow_all' | 'priority' | 'deny'`,
but `pkg/authz/permissions.go:permissionFromString` recognizes `"allow_all"`, `"priority"`
and **`"denied"`**. A row storing `'deny'` silently becomes `Unspecified` with a
`Unknown permission in DB` warning.

`authz_addresses` — the legacy wallet-keyed table (migrations `20220429002714` and
`20230905000435`): `id SERIAL PK`, `created_at TIMESTAMPTZ DEFAULT NOW()`,
`deleted_at TIMESTAMPTZ`, `wallet_address TEXT NOT NULL`, `permission TEXT NOT NULL`
(`'allow'|'deny'`), `comment TEXT`. Indexes `unique_wallet_address` (partial, on
`deleted_at IS NULL`), `authz_addresses_deleted_at`, `authz_addresses_permission`.
**No Go code reads it.**

### 5.8 messages schema (legacy, out of scope)

One table, `message` (`pkg/migrations/messages`):
`id BYTEA`, `receiverTimestamp BIGINT NOT NULL`, `senderTimestamp BIGINT NOT NULL`,
`contentTopic TEXT NOT NULL`, `pubsubTopic TEXT NOT NULL`, `payload BYTEA`,
`version INTEGER NOT NULL DEFAULT 0`, `should_expire BOOLEAN DEFAULT FALSE`,
PK `messageIndex (senderTimestamp, id, pubsubTopic)`. Surviving indexes:
`message_sendertimestamp_idx`, `message_contenttopic_idx`,
`message_recvts_shouldexpiretrue_idx` (partial, `WHERE should_expire IS TRUE`),
`message_ctopic_sts_id_idx`, `message_receivertimestamp_idx`. No sequences. This table backs
the v1/v2 message API only.

---

## 6. Ordering model: how per-topic sequence ids are produced

This is the most important behavior for a new backend to reproduce.

Every message table has a single global `BIGSERIAL` `id`. There is no per-topic sequence
column. Total order **per topic** is produced by taking a Postgres transaction advisory
lock keyed on the topic before the insert, so that the sequence value assignment and the
row's commit order agree for a given topic.

`pkg/migrations/mls/20250710145538_add-sender-hmac.up.sql` defines:

```sql
CREATE OR REPLACE FUNCTION insert_group_message_v3(group_id BYTEA, data BYTEA,
    group_id_data_hash BYTEA, sender_hmac BYTEA, should_push BOOLEAN, is_commit BOOLEAN)
    RETURNS SETOF group_messages AS $$
BEGIN
    -- Ensures that the generated sequence ID matches the insertion order
    -- Only released at the end of the enclosing transaction
    PERFORM pg_advisory_xact_lock(hashtext('group_messages_sequence'),
                                  hashtext(encode(group_id, 'hex')));
    RETURN QUERY INSERT INTO group_messages(group_id, data, group_id_data_hash,
        sender_hmac, should_push, is_commit)
      VALUES(group_id, data, group_id_data_hash, sender_hmac, should_push, is_commit)
    RETURNING *;
END; $$ LANGUAGE plpgsql;
```

The same shape exists for:

- `insert_welcome_message` / `insert_welcome_message_v4` — lock key
  `('welcome_messages_sequence', hex(installation_key))`.
- `insert_welcome_pointer_message_v1` — same welcome lock key
  (`pkg/migrations/mls/20250926170348_add_welcome_pointer_support.up.sql`).
- `insert_inbox_log` — lock key `('inbox_log_sequence', hex(inbox_id))`
  (`pkg/migrations/mls/20240829001344_serial-ids.up.sql`).
- `insert_commit_log_v2` — lock key `('commit_log_sequence', hex(group_id))`
  (`pkg/migrations/mls/20250730004640_modify_commit_log.up.sql`).

Consequences a new backend must match:

1. **Ids are globally monotonic across topics**, not per topic. A client cursor is a global
   `id` value that happens to be filtered by topic.
2. **Within one topic, `id` order equals commit order.** Across topics, a lower `id` may
   commit after a higher `id`.
3. The live poller and the XIP-83 catch-up ceiling both rely on a stronger, implicit
   assumption the code calls the "id-visibility-order invariant": rows become visible to
   readers in `id` order stream-wide. `pkg/mls/api/v1/subscribe.go:catchUpGroups` states this
   explicitly in a comment and notes it is "a pre-existing v3 property", not something the
   advisory lock guarantees globally. A new backend should treat this as a real requirement
   and probably provide it more strongly (for example a per-topic sequence).
4. Because the advisory lock is `xact`-scoped, it is held until the enclosing transaction
   commits. `pkg/mls/store/store.go:InsertGroupMessage` runs the insert as a bare
   `s.queries.InsertGroupMessage(ctx, ...)` (implicit single-statement transaction), so the
   lock is held only for that statement.
5. **The lock key is a 32-bit hash, so unrelated topics can collide.** Both key halves are
   `hashtext(...)`, which returns `int4`
   (`pkg/migrations/mls/20250710145538_add-sender-hmac.up.sql`,
   `pkg/migrations/mls/20240829001344_serial-ids.up.sql`). Two different group ids whose hex
   strings hash to the same `int4` share one lock and serialize against each other, and the
   namespace half (`'group_messages_sequence'`, `'welcome_messages_sequence'`,
   `'inbox_log_sequence'`, `'commit_log_sequence'`) can collide the same way. The guarantee
   is therefore "topic serialization **plus** possible extra serialization from hash
   collisions" — never less serialization than required, but occasionally more, which costs
   throughput rather than correctness. A new backend using a per-topic sequence avoids this
   entirely.

Deduplication is by a content hash, not by id:

- `pkg/mls/store/store.go:InsertGroupMessage` computes
  `sha256.Sum256(append(groupId, data...))` into `group_id_data_hash`.
- `pkg/mls/store/store.go:InsertWelcomeMessage` computes
  `sha256.Sum256(append(installationId, data...))` into `installation_key_data_hash`.
- `pkg/mls/store/store.go:InsertWelcomePointerMessage` computes the same hash by streaming
  `installationKey` then `welcomePointerData` into a `sha256.New()`.

A unique index on that hash column turns a repeat publish into a duplicate-key error, which
the store maps to `AlreadyExistsError` by string matching
`"duplicate key value violates unique constraint"`
(`pkg/mls/store/store.go:InsertGroupMessage` and siblings;
`pkg/mls/store/store.go:IsAlreadyExistsError`). Callers treat that as success (idempotent
publish).

---

## 7. MLS validation service (external gRPC dependency)

`pkg/mlsvalidate/service.go` speaks `xmtp.mls_validation.v1.ValidationApi` over plaintext
gRPC to `--mls-validation.grpc-address` (`pkg/mlsvalidate/config.go:MLSValidationOptions`).
The connection is created with `grpc.DialContext(..., insecure.NewCredentials())` in
`pkg/mlsvalidate/service.go:NewMlsValidationService`.

Four calls are used by the API:

| Go method | RPC | Used by |
| --- | --- | --- |
| `ValidateInboxIdKeyPackages` | `ValidateInboxIdKeyPackages` | RegisterInstallation, UploadKeyPackage |
| `ValidateGroupMessages` | `ValidateGroupMessages` | SendGroupMessages, is_commit backfiller |
| `GetAssociationState` | `GetAssociationState` | PublishIdentityUpdate |
| `VerifySmartContractWalletSignatures` | `VerifySmartContractWalletSignatures` | IdentityApi.VerifySmartContractWalletSignatures (pass-through) |

What each returns to the node:

- `ValidateInboxIdKeyPackages` (`pkg/mlsvalidate/service.go:ValidateInboxIdKeyPackages`)
  sends each key package as `ValidateKeyPackagesRequest_KeyPackage{
  KeyPackageBytesTlsSerialized: kp, IsInboxIdCredential: true}`. For each response it
  requires `IsOk`; otherwise it returns a Go error
  `validation failed with error <ErrorMessage>`. On success it yields
  `InboxIdValidationResult{InstallationKey, Credential: nil, Expiration}`. The node uses only
  `InstallationKey` (the validator derives the installation id from the key package, so the
  client does not choose it) and ignores `Expiration`.
- `ValidateGroupMessages` (`pkg/mlsvalidate/service.go:ValidateGroupMessages`) sends
  `GroupMessageBytesTlsSerialized = input.GetV1().Data` for each message. Each response must
  be `IsOk`, else the same Go error text. On success it yields
  `GroupMessageValidationResult{GroupId string, IsCommit bool}`. **`GroupId` is a hex string**
  and the node hex-decodes it (`pkg/mls/api/v1/service.go:SendGroupMessages`). The validator
  is what determines the group id and the commit flag; the client never supplies either on
  the send path.
- `GetAssociationState` (`pkg/mlsvalidate/service.go:GetAssociationState`) takes the existing
  identity updates plus the new one and returns `AssociationState` and `StateDiff`
  (`NewMembers`, `RemovedMembers`).

The node performs no MLS cryptographic checks itself. Everything protocol-level (key package
validity, message well-formedness, group id extraction, commit classification, identity
update association rules, smart-contract signature verification) happens in the validation
service.

---

## 8. MLS API endpoints

All methods live on `pkg/mls/api/v1/service.go:Service` unless noted. `Subscribe` is in
`pkg/mls/api/v1/subscribe.go`.

Two batch limits are declared at the top of `pkg/mls/api/v1/service.go`:

```go
maxBatchInserts = 10   // max entries in BatchPublishCommitLog
maxBatchQueries = 20   // max entries in BatchQueryCommitLog
```

The comment there says decreasing them would be a breaking change, because clients know
them.

`maxPageSize = 100` is declared in `pkg/mls/store/store.go`.

---

### 8.1 SendGroupMessages

`pkg/mls/api/v1/service.go:SendGroupMessages`.
RPC `/xmtp.mls.api.v1.MlsApi/SendGroupMessages`. HTTP POST `/mls/v1/send-group-messages`.
Response type is `google.protobuf.Empty`.

#### Request

`SendGroupMessagesRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `messages` | repeated `GroupMessageInput` | The messages to publish. |

`GroupMessageInput` is a oneof with only `v1` (`GroupMessageInput_V1`):

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `data` | bytes | Raw TLS-serialized MLS message. Not wrapped in any further protobuf. |
| 2 `sender_hmac` | bytes | Opaque; stored verbatim, used by push routing. Not validated. |
| 3 `should_push` | bool | Opaque flag; stored verbatim. Not validated. |

The client does **not** send a group id. The group id comes back from the validation
service as a hex string.

#### Validation

1. Publish gate: `isPublishDisabled()` — see §4.
2. `pkg/mls/api/v1/service.go:validateSendGroupMessagesRequest`:
   - `req == nil || len(req.Messages) == 0` → error.
   - any `input == nil || input.GetV1() == nil` → error.
   - **There is no maximum on `len(req.Messages)` here.** The only bound is the rate
     limiter, whose cost equals `len(req.Messages)`
     (`pkg/api/interceptor.go:applyLimits`), and `MaxRecvMsgSize` (50 MiB default).
3. `s.validationService.ValidateGroupMessages(ctx, req.Messages)` — all messages in one
   validator call. Any failure fails the whole request.
   - **There is no check that `len(validationResults) == len(req.Messages)`.** The handler
     iterates `for i, result := range validationResults` and indexes `req.Messages[i]`
     (`pkg/mls/api/v1/service.go:SendGroupMessages`). Two consequences a new backend must not
     copy: a validator returning **fewer** results than messages silently publishes only the
     prefix and still returns OK — including OK with **zero** inserts for an empty result
     slice; a validator returning **more** results than messages panics with an
     index-out-of-range on `req.Messages[i]`. Compare `RegisterInstallation`, which does
     check its result count (`pkg/mls/api/v1/service.go:RegisterInstallation`).
4. Per message, `pkg/mls/api/v1/service.go:requireReadyToSend(result.GroupId, input.GetV1().Data)`:
   - empty group id (as returned by the validator) → error.
   - empty `data` → error.
5. `hex.DecodeString(result.GroupId)` must succeed.

#### Storage

`pkg/mls/store/store.go:InsertGroupMessage` → sqlc query `InsertGroupMessage`
(`pkg/mls/store/queries.sql`), which calls
`insert_group_message_v3(group_id, data, group_id_data_hash, sender_hmac, should_push,
is_commit)`.

Columns written to `group_messages`: `group_id` (decoded from the validator's hex),
`data`, `group_id_data_hash` = `sha256(group_id || data)`, `sender_hmac`, `should_push`,
`is_commit` (from the validator). `id` and `created_at` are assigned by the table defaults.

**Transaction boundary:** each message is inserted in its own implicit transaction, in a
sequential `for` loop. The code carries a `// TODO: Wrap this in a transaction so publishing
is all or nothing` comment. A partial failure therefore leaves earlier messages persisted.

`metrics.EmitMLSSentGroupMessage` is called per inserted message.

#### Response

`google.protobuf.Empty`. The assigned `id` is **not** returned to the publisher. The
publisher learns the id only by reading the message back (query or subscription).

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled (flag or cutover) | `Unavailable` | `publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N.` |
| Nil request or no messages | `InvalidArgument` | `no group messages to send` |
| A message is nil or has no `v1` | `InvalidArgument` | `invalid group message` |
| Validation service call failed or reported not-ok | `InvalidArgument` | `invalid group message: <err>` (the wrapped error text for a not-ok response is `validation failed with error <validator message>`) |
| Validator returned empty group id | `InvalidArgument` | `group id is empty` |
| Empty `data` | `InvalidArgument` | `message is empty` |
| Group id from validator is not hex | `InvalidArgument` | `invalid group id` |
| Duplicate (unique constraint on `group_id_data_hash`) | — | **Not an error.** The loop `continue`s and the request still returns OK. |
| Validator returned **fewer** results than messages | — | **Not an error.** Only the prefix is published; the request returns OK. An empty result slice returns OK with zero inserts. |
| Validator returned **more** results than messages | — | **Panic** (index out of range on `req.Messages[i]`); the gRPC layer surfaces it as `Internal`. |
| Other insert error | `Internal` | `failed to insert message: <err>` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` |
| Unsupported libxmtp version (production only) | `Unimplemented` | `unsupported libxmtp version <v>` |

Note the code comment `// TODO: Separate validation errors from internal errors` — a
validator that is down also surfaces as `InvalidArgument`, not `Unavailable`.

#### Limits

- No batch size cap in the handler.
- Rate limit cost = number of messages, bucket type `PUB` (§13).
- Receive size cap = `--api.max-msg-size` (default 50 MiB).
- **The 4 MiB validator payload cap is enforced only in the is_commit backfiller, not on
  this path.** `maxPayloadSize = 4 * 1024 * 1024` is a constant in
  `pkg/mls/store/backfiller_group_messages.go`, used by
  `backfiller_group_messages.go:classifyMessages` to split its own batches and to reject a
  single oversized message. `SendGroupMessages` sends the whole batch to the validator in
  one call with **no local size check and no split**
  (`pkg/mls/api/v1/service.go:SendGroupMessages`). The backfiller's comment states the
  validation service itself enforces 4 MiB and that real messages reach ~3.5 MiB, so an
  oversized send batch presumably fails at the validator — but that limit is **external to
  this repository and is not verified here**. Do not read the 4 MiB number as a send-path
  limit this service applies.

#### Notes

- `is_commit` is decided by the validation service, never by the client.
- The message becomes visible to subscribers only after the DB poller sees it (§12). 25 ms
  is the poll **cadence**, not a floor: a row that commits just before a ticker event is read
  almost immediately, so the added wait is roughly uniform over 0–25 ms
  (`pkg/mls/api/v1/worker.go:DEFAULT_POLL_INTERVAL`, `listenForGroupMessages`).

---

### 8.2 SendWelcomeMessages

`pkg/mls/api/v1/service.go:SendWelcomeMessages`.
RPC `/xmtp.mls.api.v1.MlsApi/SendWelcomeMessages`. HTTP POST `/mls/v1/send-welcome-messages`.
Response `google.protobuf.Empty`.

#### Request

`SendWelcomeMessagesRequest.messages` is repeated `WelcomeMessageInput`, a oneof of two
shapes.

`WelcomeMessageInput.v1` (`WelcomeMessageInput_V1`):

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `installation_key` | bytes | Recipient installation public key. Raw bytes, not hex. This is the topic key. |
| 2 `data` | bytes | Encrypted welcome payload. |
| 3 `hpke_public_key` | bytes | HPKE public key used for the wrapper. |
| 4 `wrapper_algorithm` | enum `WelcomeWrapperAlgorithm` | `UNSPECIFIED=0`, `CURVE25519=1`, `XWING_MLKEM_768_DRAFT_6=2`, `SYMMETRIC_KEY=3`. |
| 7 `welcome_metadata` | bytes | Opaque metadata; stored verbatim. Field number is 7 (5 and 6 are unused in the input message). |

`WelcomeMessageInput.welcome_pointer` (`WelcomeMessageInput_WelcomePointer`):

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `installation_key` | bytes | Recipient installation key. |
| 2 `welcome_pointer` | bytes | Serialized `WelcomePointer` payload; stored in the `data` column. |
| 3 `hpke_public_key` | bytes | Required (non-empty). |
| 4 `wrapper_algorithm` | enum `WelcomePointerWrapperAlgorithm` | Only `XWING_MLKEM_768_DRAFT_6 = 2` is accepted. |

#### Validation

1. Publish gate `isPublishDisabled()`.
2. `pkg/mls/api/v1/service.go:validateSendWelcomeMessagesRequest`:
   - nil request or empty `messages` → error.
   - nil element → error.
   - For a `v1` input: reject when `len(data) == 0`, or `len(installation_key) == 0`, or
     (`len(hpke_public_key) == 0` **and** `wrapper_algorithm != SYMMETRIC_KEY`). So a
     symmetric-key welcome may omit the HPKE key; every other algorithm must supply it.
     `wrapper_algorithm` itself is not otherwise range-checked; unknown values silently map
     to `AlgorithmCurve25519` in
     `pkg/types/wrapperAlgorithm.go:WrapperAlgorithmFromProto`.
   - For a `welcome_pointer` input: reject when any of `installation_key`,
     `welcome_pointer`, `hpke_public_key` is empty; then
     `types.WelcomePointerWrapperAlgorithmFromProto` must succeed (only
     `XWING_MLKEM_768_DRAFT_6`).
   - Neither variant set → error.
3. **No MLS validation service call.** Welcome payloads are stored opaquely.
4. No cap on `len(req.Messages)` in the handler.

#### Storage

The handler runs all inserts **concurrently** in an `errgroup`
(`pkg/mls/api/v1/service.go:SendWelcomeMessages`), inside a tracing span
`send-welcome-messages` with tag `message_count`.

- v1 → `pkg/mls/store/store.go:InsertWelcomeMessage` → sqlc `InsertWelcomeMessage` →
  `insert_welcome_message_v4(installation_key, data, installation_key_data_hash,
  hpke_public_key, wrapper_algorithm, welcome_metadata)`. `message_type` defaults to 0.
- pointer → `pkg/mls/store/store.go:InsertWelcomePointerMessage` → sqlc
  `InsertWelcomePointerMessage` → `insert_welcome_pointer_message_v1(...)`, which writes
  `message_type = 1` and stores `welcome_pointer_data` in the `data` column.

`wrapper_algorithm` is persisted as a `SMALLINT` using the Go enum ordering in
`pkg/types/wrapperAlgorithm.go`: `AlgorithmCurve25519=0`, `AlgorithmXwingMlkem768Draft6=1`,
`AlgorithmSymmetricKey=2`. **These DB values differ from the proto enum values** (proto
CURVE25519=1, XWING=2, SYMMETRIC_KEY=3). The file carries a
`DO NOT MODIFY THE ORDER OF THESE VALUES` comment. A new backend must keep the same mapping
if it shares the database.

`InsertWelcomePointerMessage` additionally rejects an empty `hpke_public_key` in Go with a
plain `errors.New("hpke public key is required")` before touching the DB.

#### Response

`google.protobuf.Empty`. Ids are not returned.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled | `Unavailable` | `publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N.` |
| Nil request / no messages | `InvalidArgument` | `no welcome messages to send` |
| Nil element | `InvalidArgument` | `invalid welcome message` |
| v1 missing data / installation key / hpke key (non-symmetric) | `InvalidArgument` | `invalid welcome message` |
| Pointer missing any of installation key, pointer, hpke key | `InvalidArgument` | `invalid welcome pointer message` |
| Pointer wrapper algorithm not XWING | `InvalidArgument` | `invalid welcome pointer wrapper algorithm: invalid welcome pointer wrapper algorithm <v>` |
| Neither `v1` nor `welcome_pointer` set | `InvalidArgument` | `invalid welcome message: missing version` |
| Same, discovered at insert time (defensive branch inside the goroutine) | `InvalidArgument` | `invalid welcome message input: missing version` |
| Pointer algorithm rejected inside the goroutine | `InvalidArgument` | `invalid welcome pointer message: <err>` |
| Duplicate hash | — | **Not an error**; the goroutine returns nil. |
| Other insert error | `Internal` | `failed to insert message: <err>` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` |

Because the inserts run in an `errgroup`, the first error cancels the group's context and
is returned; other inserts may or may not have completed. There is no atomicity across the
batch.

#### Limits

- No handler-level batch cap. Rate-limit cost = `len(req.Messages)`, type `PUB`.

---

### 8.3 RegisterInstallation (deprecated)

`pkg/mls/api/v1/service.go:RegisterInstallation`. The doc comment says
`DEPRECATED: Use UploadKeyPackage instead`.

#### Request

`RegisterInstallationRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `key_package` | `KeyPackageUpload` | `key_package_tls_serialized` bytes. |
| 2 `is_inbox_id_credential` | bool | **Ignored.** The handler always calls `ValidateInboxIdKeyPackages`, which sets `IsInboxIdCredential: true` unconditionally (`pkg/mlsvalidate/service.go:makeValidateKeyPackageRequest`). |

#### Validation

1. `isPublishDisabled()`.
2. `validateRegisterInstallationRequest` — rejects nil request or nil `key_package`. It does
   **not** check that the serialized bytes are non-empty.
3. `ValidateInboxIdKeyPackages` with a one-element slice.
4. `len(results) != 1` is an internal error.

#### Storage

`pkg/mls/store/store.go:CreateOrUpdateInstallation` runs in an explicit transaction
(`RunInTx`) and performs two writes:

- sqlc `CreateOrUpdateInstallation`: `INSERT INTO installations(id, created_at, updated_at,
  key_package, is_appended) VALUES (...) ON CONFLICT (id) DO UPDATE SET key_package = ...,
  updated_at = ...`. `id` is the installation key returned by the validator.
  `created_at`/`updated_at` are `nowNs()` = `time.Now().UTC().UnixNano()` (a BIGINT of
  nanoseconds, not a timestamp column). `is_appended` is set to `true`.
- sqlc `InsertKeyPackage`: `INSERT INTO key_packages(installation_id, key_package,
  created_at) ... ON CONFLICT (installation_id, key_package) DO NOTHING`.

So `installations` holds the newest ("last resort") key package per installation and
`key_packages` accumulates every distinct key package ever uploaded.

#### Response

`RegisterInstallationResponse{ installation_key: bytes }` — the key the validator derived.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled | `Unavailable` | `publishing to XMTP V3 is no longer available...` |
| Nil request or nil key package | `InvalidArgument` | `no key package` |
| Validation failed / validator unreachable | `InvalidArgument` | `invalid identity: <err>` |
| Validator returned != 1 result | `Internal` | `unexpected number of results: %d` |
| DB error | — | The raw Go error is returned unwrapped, so gRPC reports `Unknown` with the driver's text. |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` |

`metrics.EmitMLSPublishedKeyPackage` is emitted with the key package byte length.

---

### 8.4 UploadKeyPackage

`pkg/mls/api/v1/service.go:UploadKeyPackage`. Response `google.protobuf.Empty`.

#### Request

`UploadKeyPackageRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `key_package` | `KeyPackageUpload` | `key_package_tls_serialized` bytes. |
| 2 `is_inbox_id_credential` | bool | **Ignored**, same as RegisterInstallation. |

#### Validation

1. `isPublishDisabled()`.
2. `validateUploadKeyPackageRequest` — nil request or nil key package rejected.
3. `ValidateInboxIdKeyPackages`.
4. The handler indexes `validationResults[0]` **without checking the length**, unlike
   RegisterInstallation. A validator that returns an empty successful response would panic
   here. Unverified whether the validator can do that; the loop in
   `pkg/mlsvalidate/service.go:ValidateInboxIdKeyPackages` sizes the output from the response
   so an empty response yields an empty slice with no error.

#### Storage

Identical to RegisterInstallation: `CreateOrUpdateInstallation` (upsert into `installations`
plus insert-if-new into `key_packages`).

#### Response

`google.protobuf.Empty`. The installation key is not echoed back.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled | `Unavailable` | `publishing to XMTP V3 is no longer available...` |
| Nil request or nil key package | `InvalidArgument` | `no key package` |
| Validation failed | `InvalidArgument` | `invalid identity: <err>` |
| DB error | `Internal` | `failed to insert key packages: <err>` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` |

Rate-limit type is `PUB` with cost 1 (`pkg/api/interceptor.go:applyLimits`).

---

### 8.5 FetchKeyPackages

`pkg/mls/api/v1/service.go:FetchKeyPackages`.

#### Request

`FetchKeyPackagesRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `installation_keys` | repeated bytes | Raw installation keys. Order is significant. |

#### Validation

**None.** There is no nil check, no empty check, and no cap on the number of keys. The
handler goes straight to the store. It also reads through the **writer** store
(`s.writerStore.FetchKeyPackages`), not the read-only store, which is inconsistent with the
other read paths.

#### Storage

sqlc `FetchKeyPackages`:

```sql
SELECT id, key_package FROM installations WHERE id = ANY (@installation_ids::BYTEA[]);
```

Only the current last-resort key package from `installations` is returned. The
`key_packages` history table is not read here.

#### Response

`FetchKeyPackagesResponse{ key_packages: repeated KeyPackage }`. The slice is
`make(..., len(ids))` and each result is placed at the index of its requested key, so:

- The response length always equals the request length.
- **A key with no installation yields a nil entry at that index** (a null element in the
  repeated field, which on the wire decodes as an absent/default message). Clients must
  handle holes positionally.
- Duplicate ids in the request: the map `keyPackageMap[string(id)]` keeps the **last**
  index for a duplicated key, so only that index is filled and the earlier duplicate slots
  stay nil.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| DB error | `Internal` | `failed to fetch key packages: <err>` |
| A returned installation id is not in the request map (should be impossible) | `Internal` | `could not find key package for installation` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |

---

### 8.6 RevokeInstallation

`pkg/mls/api/v1/service.go:RevokeInstallation` returns
`status.Error(codes.Unimplemented, "unimplemented")` unconditionally. The request type
`RevokeInstallationRequest{installation_key, wallet_signature}` exists in the proto and the
HTTP route `/mls/v1/revoke-installation` is registered, but there is no behavior. Revocation
is done through the Identity API instead (an identity update that removes a member).

| Condition | gRPC code | Message |
| --- | --- | --- |
| Always | `Unimplemented` | `unimplemented` |

---

### 8.7 GetIdentityUpdates (MLS API, legacy)

`pkg/mls/api/v1/service.go:GetIdentityUpdates` returns
`status.Error(codes.Unimplemented, "unimplemented")` unconditionally. This is the old
wallet-address-keyed identity update API (`GetIdentityUpdatesRequest{account_addresses,
start_time_ns}`). Clients must use `xmtp.identity.api.v1.IdentityApi/GetIdentityUpdates`
(§10.2).

| Condition | gRPC code | Message |
| --- | --- | --- |
| Always | `Unimplemented` | `unimplemented` |

---

### 8.8 QueryGroupMessages

`pkg/mls/api/v1/service.go:QueryGroupMessages` delegates to
`pkg/mls/store/readStore.go:QueryGroupMessagesV1` (via `s.writerStore.QueryGroupMessagesV1`,
which forwards to the read store — `pkg/mls/store/store.go:QueryGroupMessagesV1`).

#### Request

`QueryGroupMessagesRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `group_id` | bytes | Raw group id. Required. |
| 2 `paging_info` | `PagingInfo` | Optional. |

`PagingInfo`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `direction` | `SortDirection` | `UNSPECIFIED=0`, `ASCENDING=1`, `DESCENDING=2`. |
| 2 `limit` | uint32 | Page size request. |
| 3 `id_cursor` | uint64 | Exclusive cursor on `id`. |

Semantics in `QueryGroupMessagesV1`:

- Default direction is **DESCENDING**. Only an explicit `ASCENDING` changes it;
  `UNSPECIFIED` and `DESCENDING` both mean descending.
- `pageSize` starts at `maxPageSize` (100). It is replaced by `paging_info.limit` only when
  `0 < limit <= 100`. A limit above 100 is silently ignored and 100 is used. A limit of 0 is
  ignored.
- `id_cursor` is applied only when non-zero. Ascending uses `id > cursor`; descending uses
  `id < cursor`. It is always exclusive.
- `id_cursor` is converted with `int64(req.PagingInfo.IdCursor)` here (no clamping), so a
  cursor above `2^63-1` wraps negative. The query is then chosen by `if idCursor > 0`
  (`pkg/mls/store/readStore.go:QueryGroupMessagesV1`), so a **wrapped-negative cursor falls
  into the no-cursor branch**: the request is served as an unfiltered first page in the
  effective direction, not as a full-history replay. The same holds for
  `QueryWelcomeMessagesV1`. This differs from the other two cursor paths — see the
  cursor-overflow summary below.

**Cursor overflow across the four paths.** Behavior for a `id_cursor`/`sequence_id` above
`2^63-1` is not uniform, and a new backend should pick one rule rather than reproduce this:

| Path | Conversion | Behavior on a wrapped-negative cursor |
| --- | --- | --- |
| `QueryGroupMessagesV1`, `QueryWelcomeMessagesV1` (§8.8, §8.9) | `int64(...)`, then gated by `if idCursor > 0` | Treated as **no cursor**; returns the first page. |
| `QueryCommitLog` (§8.13) | `int64(...)`, used unconditionally | `id > <negative>` matches every row, so it **replays the group's commit log from the start**. |
| `GetInboxLogs` (§10.2) | `int64(req.SequenceId)` into the JSON filter | `a.sequence_id > <negative>` likewise **replays the whole inbox log**. |
| XIP-83 wave scans (§9.11) | `pkg/mls/store/readStore.go:clampCursor` | **Saturated at `2^63-1`**; nothing is replayed. Only this path clamps. |

#### Validation

`len(req.GroupId) == 0` → plain Go `errors.New("group is required")`, surfaced as gRPC
`Unknown` (the handler does not wrap it in a `status`).

#### Storage / queries

`pkg/mls/store/queries.sql`:

- No cursor: `QueryGroupMessages` — `WHERE group_id = @group_id ORDER BY CASE WHEN
  @sort_desc THEN id END DESC, CASE WHEN @sort_desc = FALSE THEN id END ASC LIMIT @numrows`.
- Ascending with cursor: `QueryGroupMessagesWithCursorAsc` — `WHERE group_id = @group_id AND
  id > @cursor ORDER BY id ASC LIMIT @numrows`.
- Descending with cursor: `QueryGroupMessagesWithCursorDesc` — `... AND id < @cursor ORDER BY
  id DESC LIMIT @numrows`.

#### Response

`QueryGroupMessagesResponse{messages, paging_info}`. Each message is a `GroupMessage.v1`:

| Field | Source |
| --- | --- |
| `id` | `group_messages.id` (uint64 of the BIGSERIAL) |
| `created_ns` | `created_at.UnixNano()` |
| `group_id` | `group_messages.group_id` |
| `data` | `group_messages.data` |
| `should_push` | `should_push.Bool` (NULL → false) |
| `sender_hmac` | `sender_hmac` |
| `is_commit` | `is_commit.Bool` (NULL → false) |

Response `paging_info`:

- `limit` = the effective page size used (not the requested one).
- `direction` = the effective direction.
- `id_cursor` = **0 unless a full page was returned**. When `len(messages) >= pageSize`, it
  is the `id` of the **last** row in the page. So a client pages by resubmitting with the
  returned cursor until `id_cursor == 0`.

This "cursor only on a full page" rule means the last page is signalled by a zero cursor,
and a page that happens to be exactly full but is also the last page costs one extra empty
round trip.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Empty group id | `Unknown` | `group is required` |
| DB error | `Unknown` | driver error text |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |

There is no `status.Error` wrapping on this path at all; every failure surfaces as
`codes.Unknown`.

#### Limits

- Max page size 100 (`pkg/mls/store/store.go:maxPageSize`), default page size also 100.
- One group id per request; there is no batched group query.

---

### 8.9 QueryWelcomeMessages

`pkg/mls/api/v1/service.go:QueryWelcomeMessages` →
`pkg/mls/store/readStore.go:QueryWelcomeMessagesV1`.

#### Request

`QueryWelcomeMessagesRequest{installation_key bytes, paging_info PagingInfo}`.

Paging semantics are identical to §8.8: default DESCENDING, page size clamped to
`(0, 100]` with default 100, exclusive `id_cursor`, `id > cursor` ascending / `id < cursor`
descending. Queries `QueryWelcomeMessages`, `QueryWelcomeMessagesWithCursorAsc`,
`QueryWelcomeMessagesWithCursorDesc`. Cursor overflow behaves as in §8.8: the `int64` cast
wraps negative and the `if idCursor > 0` gate then serves the request as a first page.

#### Validation

`len(req.InstallationKey) == 0` → `errors.New("installation is required")` (surfaces as
`Unknown`).

#### Response

`QueryWelcomeMessagesResponse{messages, paging_info}`. Each row is mapped by
`welcome_messages.message_type`:

- `message_type = 0` → `WelcomeMessage.v1` with `id`, `created_ns`, `data`,
  `installation_key`, `hpke_public_key`, `wrapper_algorithm` (DB int16 mapped back through
  `types.WrapperAlgorithmToProto`), `welcome_metadata`.
- `message_type = 1` → `WelcomeMessage.welcome_pointer` with `id`, `created_ns`,
  `installation_key`, `welcome_pointer` (from the `data` column), `hpke_public_key`,
  `wrapper_algorithm` (mapped through
  `types.WrapperAlgorithmToWelcomePointerWrapperAlgorithm`).
- Any other `message_type` → **the row is silently skipped**. The output slice is built with
  `append`, so `len(out)` can be smaller than the number of rows read.

Important pagination interaction: the response `paging_info.id_cursor` is computed from
`len(messages)` — the **raw** row count, not `len(out)` — and uses
`messages[len(messages)-1].ID`. So a skipped row still advances the cursor correctly, but a
client counting returned messages cannot infer page fullness. A new backend should keep the
cursor derived from raw rows.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Empty installation key | `Unknown` | `installation is required` |
| DB error | `Unknown` | driver text |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` |

---

### 8.10 SubscribeGroupMessages

`pkg/mls/api/v1/service.go:SubscribeGroupMessages`. Server-streaming RPC; response stream of
`GroupMessage`.

#### Request

`SubscribeGroupMessagesRequest{filters: repeated Filter}` where
`Filter{group_id bytes, id_cursor uint64}`.

- `id_cursor` is the exclusive starting point for the historical catch-up. `0` means from
  the beginning of the group's history.
- **There is no validation of the filters at all** on this method: no nil check on `req`, no
  empty-filters check, no empty-group-id check. An empty filter list produces a stream that
  is subscribed to nothing and only ever ends on disconnect or cutover.
- There is no cap on the number of filters.

#### Behavior

1. Cutover gate: `isStreamingDisabled()`.
2. `stream.SendHeader(metadata.Pairs("subscribed", "true"))` is sent first, with the code
   comment that this works around a Tonic gRPC client issue
   (<https://github.com/xmtp/libxmtp/pull/58>).
3. A dispatcher subscription is opened for the topic set **before** the historical fetch
   starts, so nothing published during catch-up is lost:
   `topic.BuildMLSV1GroupTopic(filter.GroupId)` per filter, then
   `s.subDispatcher.Subscribe(topicMap)`.
4. For each filter, a goroutine runs `fetchHistorical`, which loops
   `QueryGroupMessagesV1` with `Direction: ASCENDING` starting from `filter.IdCursor`,
   sending each page, and stops when the page is empty or the response `paging_info.IdCursor`
   is 0. Historical reads use `s.readOnlyStore` (the replica), while the live path comes from
   the dispatcher.
5. `wg.Wait()` blocks until every filter's catch-up finishes. Only then does the loop start
   forwarding live messages.
6. The live loop forwards envelopes from `sub.MessagesCh`, unmarshalling each into a
   `GroupMessage`.

#### De-duplication and ordering

`sendToStream` holds a mutex and keeps `highWaterMarks[groupID]`. A message is sent only
when `highWaterMarks[groupID] < msg.Id`; otherwise it is skipped. So:

- Per group, ids delivered are strictly increasing.
- Across groups there is no ordering guarantee.
- Live messages that arrive during catch-up sit in the dispatcher channel and are drained
  after the catch-up, and the high-water mark suppresses the ones the catch-up already sent.

#### Errors and termination

| Condition | gRPC code | Message |
| --- | --- | --- |
| Cutover passed at start, or detected by the 5 s ticker mid-stream | `Unavailable` | `XMTP V3 streaming is no longer available. Please upgrade your client to XMTP D14N.` |
| Service shutting down (`s.ctx` cancelled) | `Unavailable` | `service is shutting down` |
| Dispatcher closed the channel because the consumer was too slow | `Aborted` | `caller did not read all messages fast enough` |
| Historical query failed | — | the raw store error (typically `Unknown`) |
| `stream.Send` failed | — | the transport error |
| Client disconnected (`stream.Context().Done()`) | — | returns `nil` (clean) |
| Parse failure of an envelope | — | logged and skipped; the stream continues |

#### Limits

- Dispatcher channel depth for this subscription: `max(1024, floor(log2(numTopics))+1)` —
  effectively always 1024, because `minBacklogBufferLength = 1024` dominates
  (`pkg/subscriptions/dispatcher.go:Subscribe`). Note the buffer sizing code computes
  `log2(len(topics)) + 1` and then clamps up to 1024, so the log2 term is dead in practice.
- Overflow behavior is **drop the stream**, not drop messages: when the channel is full the
  dispatcher `close(subscription.MessagesCh)` and deletes the subscription
  (`pkg/subscriptions/dispatcher.go:HandleEnvelope`).
- Rate limit: type `DEF`, cost 1, charged once at stream start
  (`pkg/api/interceptor.go:Stream`).

---

### 8.11 SubscribeWelcomeMessages

`pkg/mls/api/v1/service.go:SubscribeWelcomeMessages`. Streams `WelcomeMessage`.

Structurally the same as §8.10, with these differences:

#### Validation (present here, absent there)

| Condition | gRPC code | Message |
| --- | --- | --- |
| `req == nil` | `InvalidArgument` | `request is nil` |
| `len(req.Filters) == 0` | `InvalidArgument` | `no valid filters provided` |
| A filter is nil or has an empty `installation_key` | `InvalidArgument` | `a filter is nil or installation key is empty` |

Note the empty-filters check happens **after** `SendHeader`, so the client sees headers then
an error.

#### Request

`SubscribeWelcomeMessagesRequest{filters}` with
`Filter{installation_key bytes, id_cursor uint64}`.

#### Behavior differences

- Topic key is `topic.BuildMLSV1WelcomeTopic(filter.InstallationKey)`.
- The historical loop additionally stops when `resp == nil` or `len(resp.Messages) == 0`,
  with debug logs, before checking the cursor.
- Errors from the historical loop are delivered through a non-blocking `sendError` helper
  (a full error channel drops the extra error) rather than a blocking send.
- The high-water map is keyed by installation key, and the id is read via
  `pkg/mls/api/v1/service.go:getMetadataFromWelcomeMessage`, which handles both `V1` and
  `WelcomePointer` variants and returns `(false, [], 0)` for a nil/unknown variant. An
  invalid message is logged and skipped.
- On a closed dispatcher channel the handler returns **`nil`** (a clean stream end), unlike
  the group version which returns `Aborted`. This is an inconsistency a new backend should
  decide about deliberately.
- Nil envelope, nil `env.Message`, parse error, and nil parsed message are each logged and
  skipped.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Streaming disabled (start or 5 s ticker) | `Unavailable` | `XMTP V3 streaming is no longer available. Please upgrade your client to XMTP D14N.` |
| Nil request | `InvalidArgument` | `request is nil` |
| No filters | `InvalidArgument` | `no valid filters provided` |
| Nil filter or empty installation key | `InvalidArgument` | `a filter is nil or installation key is empty` |
| Service shutting down | `Unavailable` | `service is shutting down` |
| Historical query error | — | raw store error |
| Dispatcher channel closed | — | `nil` (stream ends OK) |

---

### 8.12 BatchPublishCommitLog

`pkg/mls/api/v1/service.go:BatchPublishCommitLog`. Response `google.protobuf.Empty`.

#### Request

`BatchPublishCommitLogRequest{requests: repeated PublishCommitLogRequest}`.

`PublishCommitLogRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `group_id` | bytes | Raw group id. Required, non-empty. |
| 2 `serialized_commit_log_entry` | bytes | Opaque serialized `PlaintextCommitLogEntry`. Required, non-empty. Not parsed by the node. |
| 3 `signature` | `RecoverableEd25519Signature` | Required to be non-nil. **The signature is never verified.** It is re-marshalled and stored as bytes. |

#### Validation

1. `isPublishDisabled()`.
2. `pkg/mls/api/v1/service.go:validateBatchPublishCommitLogRequest`:
   - nil request or empty `requests` → error.
   - `len(req.Requests) > maxBatchInserts` (10) → error.
3. Per entry: nil entry, nil/empty `group_id`, nil/empty `serialized_commit_log_entry`, or
   nil `signature` → error.
4. `pb.Marshal(entry.Signature)` must succeed.

No cryptographic verification and no group membership check happen. Anyone who can reach the
endpoint can append to any group's commit log.

#### Storage

`pkg/mls/store/store.go:InsertCommitLog` → sqlc `InsertCommitLogV2` →
`insert_commit_log_v2(group_id, serialized_entry, serialized_signature)`, which takes the
per-group advisory lock and inserts into `commit_log_v2`. Entries are appended in a
sequential loop, one implicit transaction each — not atomic across the batch.

#### Response

`google.protobuf.Empty`. The assigned `id` (which becomes `sequence_id` on read) is not
returned.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled | `Unavailable` | `publishing to XMTP V3 is no longer available...` |
| Nil request or empty list | `InvalidArgument` | `no log entries to publish` |
| More than 10 entries | `InvalidArgument` | `cannot exceed 10 inserts in single batch` |
| Nil entry / empty group id / empty entry / nil signature | `InvalidArgument` | `invalid commit log entry` |
| Signature marshal failed | `InvalidArgument` | `invalid signature` |
| DB error | `Internal` | `failed to insert commit log: <err>` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1 — commit log publishes are **not** classified as `PUBLISH` in `pkg/api/interceptor.go:applyLimits`) |

There is no duplicate detection: `commit_log_v2` has no unique constraint on content, so the
same entry published twice yields two rows with different ids.

---

### 8.13 BatchQueryCommitLog

`pkg/mls/api/v1/service.go:BatchQueryCommitLog`.

#### Request

`BatchQueryCommitLogRequest{requests: repeated QueryCommitLogRequest}`, where
`QueryCommitLogRequest{group_id bytes, paging_info PagingInfo}`.

#### Validation

1. `pkg/mls/api/v1/service.go:validateBatchQueryCommitLogRequest`:
   - nil request or empty `requests` → error.
   - `len(req.Requests) > maxBatchQueries` (20) → error.
2. Per entry: nil entry or nil/empty `group_id` → error.
3. In `pkg/mls/store/readStore.go:QueryCommitLog`:
   - empty group id → `errors.New("group is required")`.
   - `paging_info.direction == DESCENDING` → `errors.New("descending direction is not
     supported")`. Only ascending (or unspecified, which is treated as ascending) works.

#### Storage

sqlc `QueryCommitLogV2`:

```sql
SELECT * FROM commit_log_v2 WHERE group_id = @group_id AND id > @cursor
ORDER BY id ASC LIMIT @numrows;
```

Note the cursor predicate is unconditional here: unlike the message queries, a zero cursor
still goes through the `id > 0` path (every real id is > 0, so this is equivalent to no
cursor). Because there is no `> 0` gate and no clamp, an `id_cursor` above `2^63-1` wraps
negative and `id > <negative>` matches every row — this endpoint **replays the group's whole
commit log** where §8.8 would return a first page. See the cursor-overflow table in §8.8.

Page size: `pageSize = paging_info.limit`; if `<= 0` or `> 100` it becomes 100.

#### Response

`BatchQueryCommitLogResponse{responses}` — one `QueryCommitLogResponse` per request, in the
same order.

`QueryCommitLogResponse`:

| Field | Source |
| --- | --- |
| 1 `group_id` | echoed from the request |
| 2 `commit_log_entries` | `CommitLogEntry{sequence_id = row.id, serialized_commit_log_entry = row.serialized_entry, signature = proto-unmarshalled row.serialized_signature}` |
| 3 `paging_info` | `limit` = effective page size, `direction` = ASCENDING, `id_cursor` = last row id **only when a full page was returned**, else 0 |

The code comment in `QueryCommitLog` calls the full-page-only cursor rule "strange behavior"
but keeps it for consistency with the message queries.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Nil request / empty list | `InvalidArgument` | `no requests to query` |
| More than 20 requests | `InvalidArgument` | `cannot exceed 20 queries in single batch` |
| Nil entry / empty group id (handler check) | `InvalidArgument` | `invalid request` |
| Empty group id (store check) | `Internal` | `failed to query commit log: group is required` |
| Descending direction requested | `Internal` | `failed to query commit log: descending direction is not supported` |
| Signature unmarshal failure on a stored row | `Internal` | `failed to query commit log: <proto error>` |
| DB error | `Internal` | `failed to query commit log: <err>` |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1 — note the cost is **not** the number of sub-requests, unlike the legacy `BatchQuery`) |

Note the mismatch: a `descending` request is a client error but is reported as `Internal`,
because the store returns a plain error that the handler wraps with
`status.Errorf(codes.Internal, "failed to query commit log: %s", err)`.

---

### 8.14 GetNewestGroupMessage

`pkg/mls/api/v1/service.go:GetNewestGroupMessage`.

#### Request

`GetNewestGroupMessageRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `group_ids` | repeated bytes | Raw group ids. Order significant. |
| 2 `include_content` | bool | When true the full message (including `data` and `sender_hmac`) is returned; when false only metadata. |

#### Validation

`pkg/mls/api/v1/service.go:validateGetNewestGroupMessageRequest` — nil request or empty
`group_ids` → error. **There is no cap on the number of group ids.**

#### Storage

Read from `s.readOnlyStore` (the replica).

- `include_content = true` → sqlc `GetNewestGroupMessage`:

  ```sql
  SELECT DISTINCT ON (group_id) id, group_id, data, created_at, should_push, sender_hmac, is_commit
  FROM group_messages WHERE group_id = ANY (@group_ids::BYTEA[])
  ORDER BY group_id, id DESC;
  ```

- `include_content = false` → sqlc `GetNewestGroupMessageMetadata`, the same query without
  `data`, `should_push` and `sender_hmac`.

`pkg/mls/store/readStore.go:GetNewestGroupMessage` and `GetNewestGroupMessageMetadata`
re-order the DB result back into the request order by building a map keyed by group id, and
leave `nil` where a group has no messages.

#### Response

`GetNewestGroupMessageResponse{responses}` — exactly one entry per requested group id, in
request order.

- Group with messages, `include_content = true` → `GroupMessage.v1` with `id`, `group_id`,
  `created_ns`, `data`, `should_push`, `sender_hmac`, `is_commit`
  (`pkg/mls/api/v1/service.go:convertNewestMessageToProto`).
- Group with messages, `include_content = false` → `GroupMessage.v1` with only `id`,
  `group_id`, `created_ns`, `is_commit`
  (`pkg/mls/api/v1/service.go:convertNewestMessageMetadataToProto`). `data`, `sender_hmac`
  and `should_push` are absent/zero.
- Group with no messages → an **empty** `GetNewestGroupMessageResponse_Response{}` (its
  `group_message` field is unset).

Duplicate group ids in the request are each filled, because the lookup is by map key, not by
consuming the result.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Nil request or no group ids | `InvalidArgument` | `no group ids provided` |
| DB error (either variant) | `Internal` | `database query failed` (the underlying error is logged, not returned) |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |

---

## 9. XIP-83 `Subscribe` bidirectional stream

`pkg/mls/api/v1/subscribe.go:Subscribe`. RPC
`/xmtp.mls.api.v1.MlsApi/Subscribe`, bidirectional streaming. **gRPC only — no HTTP route.**

This is the endpoint the new backend most needs to reproduce exactly. This section
describes it in full.

### 9.1 Purpose and shape

One long-lived stream carries both group and welcome messages. The client changes its
subscription set in place with `Mutate` frames instead of reconnecting. A WebSocket-style
ping/pong detects silent death in both directions.

### 9.2 Wire topics

Topics on this RPC are **kind-prefixed byte strings**, not the `/xmtp/mls/1/...` strings used
internally. `pkg/mls/api/v1/subscribe.go` defines:

```go
topicKindGroupMessagesV1   = 0x00 // identifier = group_id
topicKindWelcomeMessagesV1 = 0x01 // identifier = installation_key
```

The comment cites XIP-49 §3.3.2: the first byte is the kind, the rest is the identifier.

`pkg/mls/api/v1/subscribe.go:splitTopic` validates:

- `len(topic) < 2` → `topic must be a kind byte plus an identifier`.
- kind not in {0x00, 0x01} → `unsupported topic kind <n>`.

`pkg/mls/api/v1/subscribe.go:buildMLSTopic` maps `(kind, id)` to the internal dispatcher
topic string: kind 0x01 → `topic.BuildMLSV1WelcomeTopic(id)` =
`/xmtp/mls/1/w-<hex(id)>/proto`; anything else → `topic.BuildMLSV1GroupTopic(id)` =
`/xmtp/mls/1/g-<hex(id)>/proto` (`pkg/topic/mls.go`). Because the kind is baked into the
string prefix, a group and a welcome with identical identifier bytes never collide.

### 9.3 Request frames

`SubscribeRequest` is a oneof with only `v1` (`SubscribeRequest_V1`), itself a oneof of
three:

**`mutate` (`SubscribeRequest_V1_Mutate`)**

| Field | Type | Meaning |
| --- | --- | --- |
| 1 `adds` | repeated `Subscription{topic bytes, id_cursor uint64}` | Topics to add, each with an exclusive replay cursor. |
| 2 `removes` | repeated bytes | Wire topics to remove. |
| 3 `history_only` | bool | When true the adds are a one-shot bounded read: no live registration, no gate, no pending buffer. |
| 4 `mutate_id` | uint64 | Correlation id echoed on the wave's frames and its `CatchupComplete`. |

**`ping` (`Ping{nonce uint64}`)** — the client probes the server; the server replies with a
`Pong` carrying the same nonce.

**`pong` (`Pong{nonce uint64}`)** — the client answers a server `Ping`.

### 9.4 Response frames

`SubscribeResponse.v1` (`SubscribeResponse_V1`) is a oneof of six:

| # | Frame | Fields |
| --- | --- | --- |
| 1 | `messages` (`SubscribeResponse_V1_Messages`) | 1 `group_messages` repeated `GroupMessage`, 2 `welcome_messages` repeated `WelcomeMessage`, 3 `mutate_id` uint64 |
| 2 | `started` (`SubscribeResponse_V1_Started`) | 1 `keepalive_interval_ms` uint32, 2 `capabilities` repeated `Capability` |
| 3 | `ping` (`Ping{nonce}`) | |
| 4 | `pong` (`Pong{nonce}`) | |
| 5 | `topics_live` (`SubscribeResponse_V1_TopicsLive`) | 1 `topics` repeated bytes (wire topics) |
| 6 | `catchup_complete` (`SubscribeResponse_V1_CatchupComplete`) | 1 `mutate_id` uint64 |

The only defined `Capability` value is `CAPABILITY_UNSPECIFIED = 0`
(`pkg/proto/mls/api/v1/mls.pb.go`). The server never populates `capabilities`
(`pkg/mls/api/v1/subscribe.go:startedFrame` sets only `KeepaliveIntervalMs`), so the field is
always empty today.

A `messages` frame carries **either** group messages **or** welcome messages, never both:
`buildGroupFrames` and `buildWelcomeFrames` are separate and callers never mix lanes in one
call (see the comment on `buildGroupFrames`).

### 9.5 `mutate_id` semantics

- `mutate_id = 0` is reserved for the **live lane**. Frames produced by the live tail always
  carry `mutate_id: 0` (`pkg/mls/api/v1/subscribe.go`, the `sub.MessagesCh` case calls
  `buildGroupFrames(..., 0)` / `buildWelcomeFrames(..., 0)`).
- A `Mutate` that contains adds **must** carry a non-zero `mutate_id`. Otherwise the stream
  fails.
- A `mutate_id` that is already in flight on this stream is rejected. This applies to any
  Mutate, including a removes-only one, because its immediate `CatchupComplete` would be
  ambiguous with the in-flight wave's.
- Reuse **after** a wave's `CatchupComplete` is legal.
- **Every Mutate is answered by exactly one `CatchupComplete` echoing its `mutate_id`.** A
  removes-only Mutate, an empty Mutate, or a Mutate whose adds were all no-ops gets an
  immediate `CatchupComplete`.

### 9.6 Frame ordering contract ("the seam")

The single-writer design guarantees, per the code comments and the completion path:

1. `Started` is the first frame on the stream, before any catch-up
   (`pkg/mls/api/v1/subscribe.go`, `send(startedFrame())` before any producer goroutine
   starts). The comment cites XIP-83 server requirement 1: proxied/buffered transports keep
   the connection open.
2. For a wave, all of its history frames (`mutate_id = W`) are delivered, then the gated
   live buffer flushed (also stamped `mutate_id = W`, since those ids sit above the scan
   ceiling), then one `TopicsLive` naming the surviving topics, then one `CatchupComplete{W}`.
   Only then do the gates open.
3. Therefore **a live frame (`mutate_id = 0`) for a wave's topic is never delivered before
   that wave's `CatchupComplete`** (XIP-83 server requirement 4).
4. Within the live lane, "the dispatcher delivers each kind in ascending id order and the
   writer sends in arrival order, so the live lane stays totally ordered per kind".
5. Within a wave, ids ascend within every turn and per topic across the wave. The comment on
   `catchUpTopicPageLimit` is explicit that only the **per-topic** ascending guarantee is
   promised across a wave — the client's replay guards and the writer's high-water dedup rely
   on that, not on a total order across topics inside the wave. The gated-live fold in
   `completeWave` sorts its merged buffers before framing, but **only within each message
   kind**: `pkg/mls/api/v1/subscribe.go:completeWave` calls `sort.Slice` separately on the
   group slice and on the welcome slice, then frames groups first and welcomes second. There
   is **no cross-kind total order**, and there cannot be one: `group_messages.id` and
   `welcome_messages.id` come from two independent Postgres sequences, so their values are
   not comparable. Read "ascending id order" throughout §9 as *ascending within one message
   kind*.

### 9.7 Concurrency model

`pkg/mls/api/v1/subscribe.go:Subscribe` is a strict **single writer**:

- The `select` loop owns every piece of mutable state: `highWaterMarks`, `catchingUp`,
  `pending`, `subscribed`, `pendingBytes`, `lastActivity`, `awaitingPong`, `pingNonce`,
  `pingSentAt`, `subscribedTopics`, `nextWave`, `waves`, `topicWave`, `halfClosed`.
- A dedicated **sender goroutine** is the only caller of `stream.Send`, fed by a channel
  `outbound` of depth `sendQueueDepth = 8`. A slow client parks the sender, not the writer,
  so the writer stays free to run the ping/pong reap.
- A dedicated **frame reader goroutine** calls `stream.Recv` and forwards into
  `requestChannel` (buffered 16).
- Catch-up **fetcher goroutines** (up to two per adds-bearing Mutate: one group, one
  welcome) query the DB and hand results over `catchUpCh` (buffered
  `catchUpChannelBuffer = 4` turns).
- There are no mutexes; serialization is by single-threadedness.

### 9.8 Writer-owned state

| Name | Type | Meaning |
| --- | --- | --- |
| `highWaterMarks` | `map[string]uint64` | Topic → last id sent. Doubles as the per-topic cursor floor. |
| `catchingUp` | `map[string]bool` | Topic → catch-up in progress (its live traffic is gated). |
| `pending` | `map[string][]*Envelope` | Live envelopes held while a topic catches up. |
| `subscribed` | `map[string]struct{}` | Topics registered on the dispatcher for live delivery. |
| `pendingBytes` | int | Sum of `len(env.Message)` across `pending`. |
| `waves` | `map[int]waveState` | In-flight catch-up waves. |
| `topicWave` | `map[string]int` | Not-yet-opened topic → owning wave. |
| `halfClosed` | bool | Client called `CloseSend`; finish in-flight waves then close. |

`waveState` (`pkg/mls/api/v1/subscribe.go`):

| Field | Meaning |
| --- | --- |
| `mutateID` | The client's `mutate_id` for this wave. |
| `scansLeft` | 1 or 2 — how many kind-scans (group, welcome) have not finished. |
| `owned` | How many topics the wave still owns; reaching 0 completes it early. |
| `topics` | Internal topic strings the wave will announce. |
| `wire` | The matching wire topics, for the `TopicsLive` frame. |

### 9.9 Mutate handling, step by step

Order matters; the code is deliberate about it
(`pkg/mls/api/v1/subscribe.go`, the `v1.GetMutate() != nil` case):

1. **Reject an unrecognized request version** first: if `req.GetV1() == nil`, the stream
   fails with `InvalidArgument: unrecognized SubscribeRequest version`. The comment cites
   XIP-83 req 10 — fail rather than silently ignore, so a forward-version client is not left
   waiting.
2. **adds with `mutate_id == 0`** → fail `InvalidArgument: a Mutate with adds requires a
   nonzero mutate_id`. Checked before any state change so the rejected frame is atomic.
3. **`len(adds) > maxMutateAdds` (100 000)** → fail `ResourceExhausted:
   adds-per-Mutate limit 100000 exceeded; split the adds across Mutates`. Checked on the raw
   adds, pre-dedup, so it is stateless.
4. **`mutate_id` collision with an in-flight wave** (non-zero only) → fail
   `InvalidArgument: mutate_id <n> is already in flight on this stream`.
5. **Parse and kind-validate every add** (pure parsing, no state changes) so a malformed add
   fails before any remove's side effects land. A bad topic → `InvalidArgument: add: <reason>`.
6. **Collapse duplicate topics within this Mutate's adds**, lowest `id_cursor` winning. This
   matters because the wave-scan SQL uses `unnest`, which would return a duplicated topic's
   rows more than once (see the note on `QueryGroupMessagesWaveScan`).
7. **Apply removes**, each through `dropTopic`. A bad remove topic → `InvalidArgument:
   remove: <reason>`. Removes run **before** adds, so a topic in both is *reset*: removed
   (clearing its cursor floor) then re-added with a fresh catch-up.
8. **Apply adds**, per topic:
   - If the topic has an in-flight `history_only` wave (has a `topicWave` entry but is
     neither `subscribed` nor `catchingUp`), reject:
     `InvalidArgument: add targets a topic with an in-flight history_only catch-up`.
   - If the topic is already live or catching up:
     - and this add is `history_only` → reject: `InvalidArgument: history_only add targets a
       topic already subscribed on this stream`. (Both directions of live/history_only overlap
       are rejected.)
     - and `id_cursor >= highWaterMarks[topic]` → **no-op**, skip (re-adding at or above the
       floor changes nothing).
     - and `id_cursor < highWaterMarks[topic]` → `dropTopic(topic)` then fall through to a
       fresh add. This is how a client rewinds a topic.
   - Set `highWaterMarks[topic] = id_cursor` — the explicit starting floor.
   - If **not** `history_only`: set `catchingUp[topic] = true` **before** `sub.Add(topic)`, so
     no live message escapes before buffering starts; record in `subscribed`; bump the
     subscribed-topics gauge.
   - Append to the per-kind catch-up list (`GroupCatchup{GroupID, IdCursor}` or
     `WelcomeCatchup{InstallationKey, IdCursor}`).
9. **Start the wave, or acknowledge immediately.** If at least one add survived, allocate
   `wave := nextWave++`, record `waveState` with `scansLeft` = number of non-empty kinds and
   `owned` = number of surviving adds, map every topic to the wave, and launch
   `go catchUpGroups(...)` and/or `go catchUpWelcomes(...)`. Otherwise send
   `CatchupComplete{mutate_id}` right away.

### 9.10 `history_only`

A `history_only` add is a one-shot bounded read:

- It never registers on the dispatcher (`sub.Add` is not called), so there is no live tail.
- It never sets `catchingUp`, so nothing is gated or buffered.
- It still gets a wave, `TopicsLive` (announcing the topic) and `CatchupComplete`.
- In `completeWave`, a topic that was never `catchingUp` has its `highWaterMarks` entry
  deleted, "or one-shot reads leak it forever".

The typical bounded-catch-up flow named in the code is: client sends a `history_only` Mutate,
then `CloseSend`; the server finishes the wave, flushes, and hangs up (see §9.15).

### 9.11 Catch-up wave scan algorithm

`pkg/mls/api/v1/subscribe.go:catchUpGroups` (and the welcome analogue
`catchUpWelcomes`):

1. **Snapshot the ceiling.** `GetLatestGroupMessageID(ctx)` — `SELECT COALESCE(max(id),0)
   FROM group_messages`. This is the newest id at wave start and pins every turn so the wave
   terminates under sustained publishing. Anything newer reaches the client through the gated
   live path and is folded in when the wave completes. This first probe runs **without** a
   scan slot, because it is one index-tail probe.
2. **Prune.** Topics whose `id_cursor >= ceiling` can contribute nothing and are dropped with
   no query. The comment notes reconnect waves are overwhelmingly fully current, so most waves
   complete right here, slot-free, without queuing behind a deep scan.
3. **Acquire a scan slot** — only if there is anything left to replay. Slots are a
   per-stream channel of capacity `catchUpMaxConcurrentScans = 4`, served FIFO. The slot is
   held from before the first wave-scan query until the wave's `done` marker is in the
   channel.
4. **Re-snapshot the ceiling under the slot.** The wait may have been long, and a fresher
   ceiling moves rows from the gated pending fold into the scan (never the reverse: the
   ceiling only grows, and every queued cursor already sits below the old ceiling). A refresh
   failure is benign.
5. **Rotate in batches.** While the queue is non-empty: take up to
   `catchUpBatchTopics = 256` topics, run one query returning up to
   `catchUpTopicPageLimit = 64` rows **per topic**, deliver everything fetched, then requeue
   (to the back) only those topics that filled their per-topic limit, with their cursor
   advanced to their own last id. Topics returning fewer than the limit are fully replayed to
   the ceiling and retire.
6. **Reserve bytes.** Before forwarding a non-empty turn, the fetcher sums payload bytes and
   calls `reserveCatchUpBytes(turnBytes)`, which parks until the turn fits the per-stream
   `catchUpMaxPendingBytes = 64 MiB` budget. A CAS-from-zero arm admits exactly one turn into
   an empty lane whatever its size, so a single oversized turn replays alone rather than
   deadlocking. The writer calls `freeCatchUpBytes` when it takes a batch.
7. **Forward `catchUpBatch{done: true}`** when the queue empties.

A turn is bounded by `256 × 64 = 16 384` rows. The comment explains this caps what a hostile
subscription (many topics, all far behind) can force into one query result.

#### The SQL

`pkg/mls/store/readStore.go:queryGroupMessagesWaveScan`:

```sql
SELECT g.id, g.created_at, g.group_id, g.data, g.is_commit, g.sender_hmac, g.should_push
FROM unnest($1::BYTEA[], $2::BIGINT[]) AS f (group_id, id_cursor)
CROSS JOIN LATERAL (
    SELECT m.id, m.created_at, m.group_id, m.data, m.is_commit, m.sender_hmac, m.should_push
    FROM group_messages m
    WHERE m.group_id = f.group_id
      AND m.id > GREATEST(f.id_cursor, $3)
      AND m.id <= $4
    ORDER BY m.id ASC
    LIMIT $5
) AS g
ORDER BY g.id ASC
```

The LATERAL shape is load-bearing: each topic is one bounded range probe of the
`(group_id, id)` index capped at `limit` rows per topic, so the result is bounded by
`len(filters) × limit` and no work is fetched twice. The comment warns that a flat join with
a global `ORDER BY id` would let the planner walk the entire id range filtering by topic,
making an empty catch-up (the common reconnect case) cost a full table pass. The outer
`ORDER BY` is also load-bearing: per-topic ascending delivery is what the dedup and the
client's replay guards rely on.

`$3` (`scanCursor`) is an additional shared floor that the rotating caller always leaves at
0.

`pkg/mls/store/readStore.go:queryWelcomeMessagesWaveScan` is the same shape over
`(installation_key, id)`.

**Cursor clamping.** `pkg/mls/store/readStore.go:clampCursor` saturates a uint64 cursor at
`2^63-1` before the int64 conversion. Without it a cursor above `2^63-1` would wrap negative
and `id > cursor` would replay the entire history. Only the wave-scan path clamps; the
`QueryGroupMessagesV1` path does not.

**Filter keys must be unique.** `unnest` preserves duplicates and the join would return a
repeated topic's rows more than once. The Subscribe handler's per-Mutate dedup (step 6 in
§9.9) is what guarantees this.

**Welcome raw-progress caveat.** `QueryWelcomeMessagesWaveScan` returns
`map[string]WaveScanTopicProgress{RawCount, LastRawID}` alongside the parsed messages,
keyed by installation key. Rows with an unknown `message_type` are skipped from the parsed
slice but still consumed their topic's `LIMIT` slot. Callers **must** advance a topic's
cursor from `LastRawID` and treat `RawCount < limit` as end-of-scan; paging by the parsed
slice would silently truncate the replay at the first skipped row.
`catchUpWelcomes` does exactly this.

### 9.12 Gating live traffic during catch-up

When a live envelope arrives for a topic with `catchingUp[t] == true`, the writer buffers it
in `pending[t]` and adds `len(env.Message)` to `pendingBytes`. If `pendingBytes` exceeds
`maxPendingBytes = 64 MiB`, the stream fails:

| Condition | gRPC code | Message |
| --- | --- | --- |
| Gated live buffer over 64 MiB | `ResourceExhausted` | `catch-up buffer exceeded; reconnect from cursor` |

Otherwise the envelope waits until `completeWave`.

`pkg/mls/api/v1/subscribe.go:completeWave`:

1. For each topic the wave still owns (`topicWave[t] == wave`; topics removed or reset
   mid-wave are skipped, having been settled by `dropTopic`):
   - Add its wire topic to the `TopicsLive` marker.
   - If it was `history_only` (never `catchingUp`), delete its high-water floor and continue.
   - Otherwise clear `catchingUp[t]`, drain `pending[t]` into per-kind slices, and return the
     bytes to `pendingBytes`.
2. Sort the merged group slice and the merged welcome slice by ascending id, **as two
   independent sorts**. Each topic's buffer is already in dispatch (= id) order, but the
   wave's replay must stay ordered across its topics *of the same kind*, so the merge is
   required before framing. Group ids and welcome ids come from separate sequences and are
   never compared; the result is ordered within each kind only (§9.6 item 5).
3. Send, in one `send(...)` call and therefore one atomic batch:
   `buildGroupFrames(groups, mutateID)`, `buildWelcomeFrames(welcomes, mutateID)`, the
   `TopicsLive` marker (omitted if no topics survived), then `CatchupComplete{mutateID}`.
4. Delete the wave.

The frame builders' high-water dedup drops anything the scan already delivered.

### 9.13 Frame packing and dedup

`buildGroupFrames(msgs, mutateID)` / `buildWelcomeFrames(msgs, mutateID)`:

- For each message, compute the topic key, and **skip** it when
  `highWaterMarks[key] >= id`. Otherwise set `highWaterMarks[key] = id`. This is the single
  dedup point across catch-up and live.
- Pack survivors into frames of at most `maxFrameBytes = 2 MiB`, measured as the sum of
  `len(data)` (for welcomes, `welcomeData(m)`, which is `v1.Data` or the pointer bytes).
  A message is **never split**, so a single message larger than 2 MiB is emitted alone in its
  own frame. The comment notes individual message size is bounded below gRPC's hard limit by
  the publish path, so a single-message frame still fits. Note this accounting ignores
  protobuf overhead and the other fields.

### 9.14 `dropTopic`

`pkg/mls/api/v1/subscribe.go:dropTopic(t)`:

1. `sub.Remove(t)` — stop live delivery.
2. `delete(highWaterMarks, t)` — **clear the per-stream cursor floor**, so a later re-add can
   replay from a lower cursor.
3. If the topic was live, remove it from `subscribed`, decrement the gauge and emit
   `metrics.EmitUnsubscribeTopics(ctx, log, 1)`. Duplicate or unknown removes are harmless
   because the gauge only moves when a genuinely live topic leaves.
4. If it was `catchingUp`, subtract its buffered bytes from `pendingBytes` and drop
   `pending[t]` and `catchingUp[t]`.
5. If it had a `topicWave` entry, decrement that wave's `owned`. If `owned` reaches 0, delete
   the wave and send its `CatchupComplete` immediately — the wave's remaining batches are then
   dropped by the writer as stragglers.

`dropTopic` runs only on the writer goroutine and never after half-close (mutations stop
then).

### 9.15 Half-close (`CloseSend`) and graceful drain

When the frame reader sees `io.EOF` (or `context.Canceled`) it records no error and closes
`requestChannel`. The writer then:

- If `recvErr != nil` (a real transport failure) → fail:
  `Unavailable: stream recv failed: <err>`.
- Else, if `len(waves) == 0` → `flush()` and return.
- Else set `halfClosed = true`, clear `awaitingPong`, set `requestChannel = nil` so that
  case goes dormant, and **stop pinging** (a half-closed peer cannot answer; the bounded
  drain plus gRPC transport timeouts cover liveness). When the last wave completes, the
  writer calls `flush()`.

`pkg/mls/api/v1/subscribe.go:flush`:

- Closes `outbound` and waits for the sender to drain.
- If the sender stopped early on a Send error, that error is returned rather than a false OK.
- If `ctx.Done()` fires (client disconnected) → return `nil`; gRPC surfaces the cancellation.
- If the drain takes longer than `pongDeadline` (30 s) → return
  `DeadlineExceeded: flush timed out waiting for sender to drain`. The comment is explicit
  that this is not a successful completion: the queued history tail and `CatchupComplete`
  were not delivered.

Only clean-completion paths call `flush`; error teardowns just return.

### 9.16 Keepalive: Ping / Pong

Constants (`pkg/mls/api/v1/subscribe.go`):

```go
subscribePingInterval = 30 * time.Second
subscribePongDeadline = subscribePingInterval   // 30s
```

Both are fields on `Service` (`pingInterval`, `pongDeadline`) so tests can override them;
production uses the constants (`pkg/mls/api/v1/service.go:NewService`).

- The `Started` frame advertises `keepalive_interval_ms = pingInterval / time.Millisecond`
  = 30000.
- A `time.Ticker` fires every `pingInterval`. On each tick:
  - If `halfClosed`, do nothing.
  - Else if `awaitingPong` and `time.Since(pingSentAt) >= pongDeadline` → fail:
    `DeadlineExceeded: no Pong within deadline`.
  - Else if `time.Since(lastActivity) >= pingInterval` → increment `pingNonce`, send
    `Ping{nonce}`, set `awaitingPong = true`, `pingSentAt = now`.
- `lastActivity` advances **only when a send batch is admitted to the outbound queue**, never
  on inbound frames. Precisely: `send(...)` sets `lastActivity = time.Now()` in the
  `case outbound <- flat:` arm (`pkg/mls/api/v1/subscribe.go`, the `send` closure), which is
  the moment the batch enters the buffered `outbound` channel — **not** the moment
  `stream.Send` returns on the sender goroutine. With `sendQueueDepth = 8`, up to eight
  batches can be queued and unsent while `lastActivity` already reflects them, so the idle
  timer measures queue admission, not delivery. The comment is emphatic that the Ping probes
  the client's *receive* path;
  inbound frames prove only the send path, so a client that streams frames but never reads
  must not be able to suppress the probe (and the reap) forever.
- Only a `Pong` whose nonce equals the current `pingNonce` clears `awaitingPong`. A stale or
  unsolicited `Pong` is ignored.
- A client `Ping{nonce}` is answered immediately with `Pong{nonce}` (echoing the same nonce).

### 9.17 Backpressure and overload

Three separate mechanisms, with different outcomes:

| Mechanism | Bound | Behavior when exceeded |
| --- | --- | --- |
| Dispatcher channel (live) | `subscribeBacklog = 4096` envelopes (`s.subDispatcher.NewSubscription(4096)`; the dispatcher clamps up to `minBacklogBufferLength = 1024`, so 4096 stands) | The dispatcher closes the channel and deletes the subscription; the writer fails the stream with `Aborted: subscription closed: consumer too slow`. |
| Gated pending buffer | `maxPendingBytes = 64 MiB` | Stream fails: `ResourceExhausted: catch-up buffer exceeded; reconnect from cursor`. |
| Catch-up fetch budget | `catchUpMaxPendingBytes = 64 MiB` fetched-but-unconsumed | **Backpressure, never a drop.** Fetchers park in `reserveCatchUpBytes` until the writer frees room. |
| Catch-up channel | `catchUpChannelBuffer = 4` turns (~65k messages at the 16 384-row turn cap) | Fetchers block on `forward`, i.e. backpressure. |
| Scan concurrency | `catchUpMaxConcurrentScans = 4` per stream | Excess scans park FIFO on the slot channel. A fully-current wave prunes slot-free and never queues. |
| Writer → sender queue | `sendQueueDepth = 8` batches | `send` blocks up to `pongDeadline`; past that the stream fails `Unavailable: send stalled; client not reading`. |

The comment on `catchUpMaxPendingBytes` notes what the budget deliberately does **not**
cover: not-yet-reserved query results in flight (up to `catchUpMaxConcurrentScans` turns) and
the writer→sender pipeline (bounded separately by `sendQueueDepth` plus the writer's and
sender's own hands).

### 9.18 Complete error table for `Subscribe`

| Condition | gRPC code | Message |
| --- | --- | --- |
| `req.GetV1() == nil` (unknown request version) | `InvalidArgument` | `unrecognized SubscribeRequest version` |
| Mutate has adds but `mutate_id == 0` | `InvalidArgument` | `a Mutate with adds requires a nonzero mutate_id` |
| `len(adds) > 100000` | `ResourceExhausted` | `adds-per-Mutate limit 100000 exceeded; split the adds across Mutates` |
| `mutate_id` already in flight (non-zero) | `InvalidArgument` | `mutate_id <n> is already in flight on this stream` |
| Add topic shorter than 2 bytes | `InvalidArgument` | `add: topic must be a kind byte plus an identifier` |
| Add topic with unknown kind | `InvalidArgument` | `add: unsupported topic kind <n>` |
| Remove topic shorter than 2 bytes | `InvalidArgument` | `remove: topic must be a kind byte plus an identifier` |
| Remove topic with unknown kind | `InvalidArgument` | `remove: unsupported topic kind <n>` |
| Add targets a topic with an in-flight history_only catch-up | `InvalidArgument` | `add targets a topic with an in-flight history_only catch-up` |
| `history_only` add targets an already-live/catching-up topic | `InvalidArgument` | `history_only add targets a topic already subscribed on this stream` |
| Catch-up DB fetch failed | `Unavailable` | `catch-up failed: <err>` |
| Gated live buffer exceeded 64 MiB | `ResourceExhausted` | `catch-up buffer exceeded; reconnect from cursor` |
| Dispatcher closed the channel (consumer too slow) | `Aborted` | `subscription closed: consumer too slow` |
| No Pong within the deadline | `DeadlineExceeded` | `no Pong within deadline` |
| `send` blocked past the pong deadline | `Unavailable` | `send stalled; client not reading` |
| Graceful flush did not drain in time | `DeadlineExceeded` | `flush timed out waiting for sender to drain` |
| Service shutting down (`s.ctx` cancelled), from `send` or the main loop | `Unavailable` | `service is shutting down` |
| Transport failure on Recv | `Unavailable` | `stream recv failed: <err>` |
| `stream.Send` failed on the sender goroutine | — | the transport error, surfaced verbatim |
| Client disconnected (`ctx.Done`) | — | `nil` (clean) |
| Rate limited at stream open | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |
| Unsupported libxmtp version (production) | `Unimplemented` | `unsupported libxmtp version <v>` |

Non-fatal, logged-and-skipped conditions: an envelope that fails to unmarshal into a
`GroupMessage` or `WelcomeMessage` (both in the live lane and in the pending fold).

**Silently ignored, and not an error:** a `SubscribeRequest` whose outer `v1` **is** present
but whose inner oneof matches none of `mutate`, `ping`, `pong`. The handler's
`switch { case v1.GetMutate() != nil: ... case v1.GetPing() != nil: ... case
v1.GetPong() != nil: ... }` has **no `default` arm**
(`pkg/mls/api/v1/subscribe.go:Subscribe`), so such a frame produces no response and no
failure — the client waits forever for an acknowledgement that never comes. This is the
opposite of the missing-outer-`v1` case in the first row of the table, which fails with
`InvalidArgument` explicitly citing XIP-83 req 10. So the fail-fast-on-unknown-version
guarantee covers only the **outer** envelope: an empty `v1`, or a `v1` carrying a
forward-version inner arm this server does not know, is dropped without a word. A new
backend should extend the same fail-fast rule to the inner oneof.

### 9.19 Limits summary for `Subscribe`

| Constant | Value | Meaning |
| --- | --- | --- |
| `subscribePingInterval` | 30 s | Ping cadence; also advertised as `keepalive_interval_ms`. |
| `subscribePongDeadline` | 30 s | Pong wait; also the `send` stall bound and the flush bound. |
| `subscribeBacklog` | 4096 | Dispatcher channel depth for this stream. |
| `sendQueueDepth` | 8 | Writer→sender frame-batch queue depth. |
| `catchUpTopicPageLimit` | 64 | Rows fetched per topic per turn. |
| `catchUpBatchTopics` | 256 | Topics queried per turn (turn cap = 16 384 rows). |
| `catchUpChannelBuffer` | 4 | Turns buffered between fetchers and writer. |
| `catchUpMaxConcurrentScans` | 4 | Concurrent scans per stream. |
| `catchUpMaxPendingBytes` | 64 MiB | Fetched-but-unconsumed catch-up payload bytes. |
| `maxMutateAdds` | 100 000 | Raw adds per Mutate (pre-dedup). |
| `maxPendingBytes` | 64 MiB | Gated live buffer. |
| `maxFrameBytes` | 2 MiB | Target server frame size (best-effort; never splits a message). |

There is **no cap on the total number of topics subscribed on a stream** — only the
per-Mutate adds cap. A client can accumulate arbitrarily many topics across many Mutates.

---

## 10. Identity API endpoints

`pkg/identity/api/v1/identity_service.go`. Service `xmtp.identity.api.v1.IdentityApi`.

### 10.1 PublishIdentityUpdate

RPC `/xmtp.identity.api.v1.IdentityApi/PublishIdentityUpdate`.
HTTP POST `/identity/v1/publish-identity-update`.

#### Request

`PublishIdentityUpdateRequest{ 1 identity_update: associations.IdentityUpdate }`.
The `IdentityUpdate` carries `inbox_id` as a **hex string** (it is decoded with
`decode(@inbox_id, 'hex')` in SQL) plus the signed actions.

#### Validation and algorithm

The handler checks `isPublishDisabled()` then delegates to
`pkg/mls/store/store.go:PublishIdentityUpdate`. The doc comment above the handler spells out
the intended properties:

> 1. Updates come in and are assigned sequence numbers in some order.
> 2. Updates are not visible to API consumers until they have been validated and the
>    address_log table has been updated.
> 3. If you read once and then read again: (a) the second read must have all updates from the
>    first; (b) new updates cannot have a lower sequence number than the latest from the
>    first; (c) this applies per inbox only.

The store implementation (`pkg/mls/store/store.go:PublishIdentityUpdate`) runs inside
`RunInRepeatableReadTx(ctx, 3, ...)` — a **REPEATABLE READ** transaction retried up to 3
times with `utils.RandomSleep(20)` between attempts. (The handler comment says SERIALIZABLE;
the code uses `sql.LevelRepeatableRead`. This is a documentation/code mismatch worth noting.)

Inside the transaction:

1. `txQueries.LockInboxLog(ctx, inboxId)` → `SELECT pg_advisory_xact_lock(hashtext(@inbox_id))`.
   The comment explains an advisory lock is used instead of `SELECT FOR UPDATE` so the lock
   works even when no `inbox_log` row exists yet. **Note this lock hashes the hex string
   `inbox_id`, whereas `insert_inbox_log` locks on
   `(hashtext('inbox_log_sequence'), hashtext(hex(inbox_id)))` — a two-key lock. They are
   different lock identifiers, so the advisory lock here serializes publishers per inbox and
   the function's lock additionally orders sequence assignment.**
2. `txQueries.GetAllInboxLogs(ctx, inboxId)` — all entries ordered by `sequence_id ASC`.
3. If `len(entries) >= 256` → `errors.New("inbox log is full")`.
4. Unmarshal every stored `identity_update_proto` into `associations.IdentityUpdate`.
5. `validationService.GetAssociationState(ctx, oldUpdates, []{newUpdate})` — the validator
   replays the whole log plus the new update and returns the resulting state and diff. Any
   error aborts.
6. `txQueries.InsertInboxLog(...)` → `SELECT sequence_id FROM insert_inbox_log(decode(inbox_id,
   'hex'), server_timestamp_ns, identity_update_proto)`. `server_timestamp_ns` is
   `nowNs()` = `time.Now().UTC().UnixNano()` computed on the node, not in the database.
7. For each `state.StateDiff.NewMembers` whose kind is `MemberIdentifier_EthereumAddress`:
   `InsertAddressLog{address, inbox_id, association_sequence_id: sequence_id,
   revocation_sequence_id: NULL}`. Non-Ethereum member kinds (for example passkeys) are
   **not** written to `address_log`.
8. For each `state.StateDiff.RemovedMembers` that is an Ethereum address:
   `RevokeAddressFromLog{address, inbox_id, revocation_sequence_id: sequence_id}`, which
   updates the row with the maximum `association_sequence_id` for that (address, inbox).
9. `txQueries.TouchInbox(ctx, inboxId)` — `INSERT INTO inboxes(id) VALUES (decode(...))
   ON CONFLICT (id) DO UPDATE SET updated_at = NOW()`.

`metrics.EmitMLSSentIdentityUpdate` records the marshalled proto size.

#### Response

`PublishIdentityUpdateResponse{}` — empty. The assigned `sequence_id` is **not** returned.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Publishing disabled | `Unavailable` | `publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N.` |
| `identity_update` missing | `Unknown` | `IdentityUpdate is required` |
| Inbox log already has 256+ entries | `Unknown` | `inbox log is full` |
| Validator rejected the update or is unreachable | **the validator's own code** | the validator's own message — see below |
| Any DB error | `Unknown` | driver text, after 3 retries |

There is no `status.Error` anywhere in `PublishIdentityUpdate`, so **locally raised** errors
(the two `errors.New` above) and driver errors surface as `codes.Unknown`.

**Validator errors are not `Unknown`.** The failure from `GetAssociationState` is a gRPC
`status` error produced by the validation service's own client stub, and nothing on the
return path rewraps it: `pkg/mlsvalidate/service.go:GetAssociationState` returns
`grpcClient.GetAssociationState`'s error verbatim; `pkg/mls/store/store.go:PublishIdentityUpdate`
returns it from the transaction closure; `pkg/mls/store/transactions.go:RunInTx` returns the
closure's error unchanged; and `pkg/identity/api/v1/identity_service.go:PublishIdentityUpdate`
logs it and returns it as-is. Because a `status` error still satisfies `GRPCStatus()`, gRPC
transmits **the validator's code and message**, not `Unknown`. So a rejected update surfaces
with whatever the validation service chose (commonly `InvalidArgument`), and a validator that
is down surfaces as `Unavailable`. A client cannot use `Unknown` to distinguish these.

#### Limits

- **256 entries per inbox log**, hard.
- Rate limit: type `PUB`, cost 1.

---

### 10.2 GetIdentityUpdates

RPC `/xmtp.identity.api.v1.IdentityApi/GetIdentityUpdates`.
HTTP POST `/identity/v1/get-identity-updates`.

#### Request

`GetIdentityUpdatesRequest{ 1 requests: repeated Request }` where
`Request{ 1 inbox_id string (hex), 2 sequence_id uint64 }`.

`sequence_id` is an **exclusive** floor: the query is `a.sequence_id > b.sequence_id`.

#### Validation

**None.** No nil check, no empty check, no cap on the number of sub-requests, no hex
validation on `inbox_id` (a bad hex value makes Postgres raise, surfacing as `Unknown`).

#### Storage

`pkg/mls/store/readStore.go:GetInboxLogs` builds a JSON array of
`{inbox_id, sequence_id}` filters (`pkg/mls/store/queries/filters.go`,
`queries.InboxLogFilterList.ToSql`) and runs sqlc `GetInboxLogFiltered`:

```sql
SELECT a.sequence_id, encode(a.inbox_id,'hex') AS inbox_id, a.identity_update_proto,
       a.server_timestamp_ns
FROM inbox_log AS a
JOIN (SELECT * FROM json_populate_recordset(NULL::inbox_filter, @filters) AS b(inbox_id, sequence_id)) AS b
  ON decode(b.inbox_id,'hex') = a.inbox_id::BYTEA AND a.sequence_id > b.sequence_id
ORDER BY a.sequence_id ASC;
```

The filters are passed as JSON and decoded inside Postgres against a composite type
`inbox_filter`. This means **the whole batch is one query**, and results are re-grouped in
Go by `inbox_id`.

**This endpoint reads the primary, not the replica.** The handler uses `s.store` — the
read-write store (`pkg/identity/api/v1/identity_service.go:GetIdentityUpdates`). That
forwards to `s.readstore.GetInboxLogs` (`pkg/mls/store/store.go:GetInboxLogs`), but the
writer store builds that internal read store over its **own** connection:
`pkg/mls/store/store.go:New` sets `readstore: NewReadStore(log, db)` with the same `db`
handle, with the comment "Create the read store with the same database connection as the
write store". The replica handle (`ReadMLSStore`, built from
`--mls-store.reader-db-connection-string` in `pkg/server/server.go` and passed to the
service as `readOnlyStore`) is **not** used here. `GetInboxIds` (§10.3) is the Identity
endpoint that does use the configured read store.

#### Response

`GetIdentityUpdatesResponse{responses}` — one `Response{inbox_id, updates}` per request, in
request order. Each `IdentityUpdateLog` carries `sequence_id`, `server_timestamp_ns` and the
unmarshalled `update`.

- Updates for one inbox are ordered by `sequence_id ASC` (the SQL orders globally by
  `sequence_id`, and grouping preserves that relative order per inbox).
- **There is no page size limit.** The full tail of an inbox log above the cursor is
  returned. The 256-entry cap on the log itself is what bounds this.

**Repeated `inbox_id` filters are not independent.** The SQL is a plain `JOIN` against the
filter recordset, so a log row matching *n* filters is returned *n* times
(`pkg/mls/store/queries.sql:GetInboxLogFiltered`). Go then merges every returned row into one
list per inbox id (`resultMap[result.InboxID] = append(...)` in
`pkg/mls/store/readStore.go:GetInboxLogs`) and hands that **same merged list** to every
response slot for that inbox. Concretely:

- Two identical filters for one inbox → each matching row appears **twice** in the merged
  list, and **both** response slots carry the duplicated list.
- Two filters for one inbox with **different** cursors → the union of both cursor ranges is
  merged into one list, and both slots get that combined list. Neither slot reflects its own
  cursor. The per-slot answer a client would expect is not produced.

A new backend should either reject repeated inbox ids or evaluate each request slot
independently.

**Inbox-id matching is case-sensitive, and the SQL canonicalizes to lowercase.** The query
returns `encode(a.inbox_id, 'hex') AS inbox_id` — Postgres `encode` always emits **lowercase**
hex — while the join matches on `decode(b.inbox_id, 'hex')`, and `decode` accepts either case.
Go keys the result map by the SQL-returned (lowercase) value but looks it up by the **raw
request string**: `resultMap[req.InboxId]` (`pkg/mls/store/readStore.go:GetInboxLogs`). So a
request whose `inbox_id` contains any uppercase hex digit **matches rows in SQL but finds
nothing in the map**, and the client receives a response slot with the correct `inbox_id`
echoed and an **empty `updates` list** — silently, with no error. Clients must send lowercase
hex. A new backend should normalize the id on both sides.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| JSON filter marshal failure | `Unknown` | marshal error |
| DB error (including bad hex) | `Unknown` | driver text |
| A stored `identity_update_proto` fails to unmarshal | `Unknown` | proto error |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1 — **not** proportional to the number of sub-requests) |

---

### 10.3 GetInboxIds

RPC `/xmtp.identity.api.v1.IdentityApi/GetInboxIds`.
HTTP POST `/identity/v1/get-inbox-ids`.

#### Request

`GetInboxIdsRequest{ 1 requests: repeated Request }` where
`Request{ 1 identifier string, 2 identifier_kind associations.IdentifierKind }`.

**`identifier_kind` is ignored.** `pkg/mls/store/readStore.go:GetInboxIds` collects only
`request.GetIdentifier()` and looks it up in `address_log.address`. There is no per-kind
namespacing in the query, so an Ethereum address and a same-string identifier of another kind
would collide. `address_log` only ever receives Ethereum addresses
(`pkg/mls/store/store.go:PublishIdentityUpdate` filters on
`MemberIdentifier_EthereumAddress`), so in practice only Ethereum lookups resolve.

#### Validation

None. No nil check, no empty check, no cap on the number of requests.

#### Storage

sqlc `GetAddressLogs`:

```sql
SELECT a.address, encode(a.inbox_id,'hex') AS inbox_id, a.association_sequence_id
FROM address_log a
INNER JOIN (
  SELECT address, MAX(association_sequence_id) AS max_association_sequence_id
  FROM address_log
  WHERE address = ANY (@addresses::TEXT[]) AND revocation_sequence_id IS NULL
  GROUP BY address) b
ON a.address = b.address AND a.association_sequence_id = b.max_association_sequence_id;
```

So: the inbox with the highest `association_sequence_id` for that address among rows whose
`revocation_sequence_id IS NULL`. A revoked association is excluded.

The handler comment describes it as "the largest association_sequence_id where
revocation_sequence_id is lower or NULL", but the SQL only matches `IS NULL`.

#### Response

`GetInboxIdsResponse{responses}` — one entry per requested identifier, in request order.

- `identifier` is echoed.
- `inbox_id` is an **optional string**: absent when no unrevoked association exists.
- `identifier_kind` is **never set on the response** (the Go code only sets `Identifier` and
  `InboxId` in `pkg/mls/store/readStore.go:GetInboxIds`), so it always decodes as the zero
  value.
- The matching loop is `for _, logEntry := range addressLogEntries { if logEntry.Address ==
  identifier { resp.InboxId = &inboxId } }` — a linear scan per identifier, and the **last**
  matching row wins. The SQL should return at most one row per address.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| DB error | `Unknown` | driver text |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |

Read path uses `s.readOnlyStore` (the replica).

---

### 10.4 VerifySmartContractWalletSignatures

RPC `/xmtp.identity.api.v1.IdentityApi/VerifySmartContractWalletSignatures`.
HTTP POST `/identity/v1/verify-smart-contract-wallet-signatures`.

#### Request

`VerifySmartContractWalletSignaturesRequest{ 1 signatures: repeated Signature }` where
`Signature{ 1 account_id string (CAIP-10), 2 block_number optional uint64, 3 signature bytes,
4 hash bytes }`.

#### Behavior

`pkg/identity/api/v1/identity_service.go:VerifySmartContractWalletSignatures` is a pure
pass-through to
`pkg/mlsvalidate/service.go:VerifySmartContractWalletSignatures`, which forwards the request
verbatim to the validation service's identical RPC. The node performs **no** validation of
its own, does no chain access, and does not touch the database.

#### Response

`VerifySmartContractWalletSignaturesResponse{ 1 responses: repeated ValidationResponse }`
where `ValidationResponse{ 1 is_valid bool, 2 block_number optional uint64,
3 error optional string }`. Returned unchanged from the validator.

#### Errors

| Condition | gRPC code | Message |
| --- | --- | --- |
| Validator returned an error or is unreachable | — | the validator's status is returned verbatim (whatever code the validation service produced) |
| Rate limited | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` (type `DEF`, cost 1) |

This endpoint is **not** gated by the cutover or `--api.disable-mls-publish`.

---

## 11. D14N migration API

`pkg/migration/api/v1/service.go`. Service `xmtp.migration.api.v1.D14nMigrationApi`.
Registered only when `--api.enable-migration` is set.

**Name casing.** The **wire** names use a lowercase `n`. The generated full method name is
the literal
`"/xmtp.migration.api.v1.D14nMigrationApi/FetchD14nCutover"`
(`pkg/proto/migration/api/v1/migration_grpc.pb.go`, constant
`D14NMigrationApi_FetchD14NCutover_FullMethodName`). The **Go** identifiers around it —
`D14NMigrationApi`, `FetchD14NCutover`, `FetchD14NCutoverResponse` — capitalize the `N`. A
client dialing this RPC by string, or a new backend registering it, must use the lowercase-`n`
wire form; the uppercase spelling appears only in Go source.

### FetchD14nCutover

Request: `google.protobuf.Empty`.
Response: `FetchD14NCutoverResponse{ 1 timestamp_ns uint64 }` — the configured
`--api.d14n-cutover-ns`, returned verbatim, with no validation and no error path. Clients use
it to learn when to switch to the decentralized backend.

---

## 12. Live delivery: DB poller and subscription dispatcher

### The DB poller

`pkg/mls/api/v1/worker.go:dbWorker`. Created by
`pkg/mls/api/v1/service.go:NewService` with `DEFAULT_POLL_INTERVAL = 25 * time.Millisecond`.

There is **no Postgres LISTEN/NOTIFY and no in-process fan-out from the write path.** Live
delivery is pure polling of the primary read handle (`s.readOnlyStore.Queries()`).

Startup (`pkg/mls/api/v1/worker.go:start` and `getStartPoints`): read
`GetLatestGroupMessageID` and `GetLatestWelcomeMessageID` — `SELECT COALESCE(max(id),0)`.
The workers start from the current tail, so **messages written before the process started are
never dispatched live**; a subscriber gets them only through catch-up.

Two goroutines, one per table
(`pkg/mls/api/v1/worker.go:listenForGroupMessages` / `listenForWelcomeMessages`), each on a
25 ms ticker:

1. `GetAllGroupMessagesWithCursor{Cursor: currentID, Numrows: 500}` —
   `SELECT * FROM group_messages WHERE id > @cursor ORDER BY id ASC LIMIT 500`. Same shape for
   welcomes.
2. Build the proto for each row. For welcomes, `message_type` 0 → `WelcomeMessage.v1`,
   1 → `WelcomeMessage.welcome_pointer`, anything else → logged
   (`unknown welcome message type`) and **skipped**.
3. `proto.Marshal`, then `subDispatcher.HandleEnvelope(&Envelope{ContentTopic:
   topic.BuildMLSV1GroupTopic(row.GroupID), Message: data, TimestampNs:
   row.CreatedAt.UnixNano()})`.
4. `currentID = <last row's id>` if any rows were returned.

Consequences a new backend should note:

- **25 ms is the poll cadence, not a latency floor.** A row that commits just before a ticker
  event is picked up on that same tick, so the wait the poller adds ranges from ~0 ms to
  25 ms depending on where the commit lands in the cycle (mean ~12.5 ms), plus DB commit
  visibility. Do not quote 25 ms as a minimum.
- Batch size 500 per tick per table caps live throughput at ~20 000 rows/second per table
  before the poller falls behind.
- The cursor advances to the last row of a page even when a row was skipped (unknown welcome
  type), so nothing is retried.
- `proto.Marshal` failure `break`s out of the select (the `SelectLoop` label), which skips the
  cursor advance for that tick and retries the same page next tick — a potential hot loop on a
  permanently unmarshallable row. Unverified whether that can happen in practice.
- The poller reads the **read-only store's queries** handle
  (`pkg/mls/api/v1/service.go:NewService` passes `s.readOnlyStore.Queries()`), so on a replica
  deployment live delivery inherits replica lag.
- Only rows appearing in `id` order are seen. A row that commits after a reader has already
  passed its id is "undeliverable stream-wide" — the pre-existing v3 property the XIP-83
  ceiling logic depends on (`pkg/mls/api/v1/subscribe.go:catchUpGroups` comment).

### The dispatcher

`pkg/subscriptions/dispatcher.go`.

- `NewSubscriptionDispatcher(log)` holds `map[*Subscription]interface{}` under one mutex.
- `HandleEnvelope(env)` takes the mutex and, for each subscription, delivers when
  `sub.all` (and the topic is a valid subscribe-all topic) or `sub.topics[env.ContentTopic]`.
- Delivery is a **non-blocking** channel send. When the channel is full, the dispatcher
  logs `Subscription message channel is full, ...`, **closes the channel and deletes the
  subscription**. It never blocks and never drops individual messages — it drops the whole
  subscriber. Consumers must treat a closed channel as "reconnect from your cursor".
- `Subscribe(topics map[string]bool)` (used by the legacy MLS subscribe methods) computes
  `backlogBufferSize = log2(len(topics)) + 1`, clamped up to `minBacklogBufferLength = 1024`.
  In practice the buffer is always 1024. A wildcard `"*"` topic makes the subscription an
  all-topics subscription with buffer `allTopicsBacklogLength = 4096`.
- `NewSubscription(bufferSize)` (used by XIP-83 `Subscribe`) creates an empty mutable
  subscription with the given buffer, clamped up to 1024.
- `Subscription.Add(topics...)` / `Remove(topics...)`
  (`pkg/subscriptions/subscription.go`) mutate the topic set in place under the dispatcher
  mutex, which is the same lock `HandleEnvelope` holds while reading — so mutation and
  dispatch are serialized.
- `isValidSubscribeAllTopic` accepts topics prefixed `/xmtp/0/` (v2) or matching
  `topic.IsMLSV1` (`/xmtp/mls/1/`).

---

## 13. Rate limiting

`pkg/ratelimiter/rate_limiter.go` and `pkg/api/interceptor.go`.
Enabled only when `--api.authn.ratelimits` is set (`pkg/api/server.go:startGRPC`).

### Algorithm

A **token bucket** with lazy refill (`pkg/ratelimiter/rate_limiter.go:Limit.Refill`):

- Each bucket entry holds `tokens uint16` and `lastSeen time.Time`, plus a per-entry mutex to
  avoid lock contention on the map.
- On access, `minutesSinceLastSeen := now.Sub(entry.lastSeen).Minutes()`. If `> 0`, set
  `lastSeen = now` and add `int(ratePerMinute) * int(minutesSinceLastSeen)` tokens, clamped
  at `MAX_UINT_16 = 65535` and then at `maxTokens`.
  - Note the multiplication uses `int(minutesSinceLastSeen)` — a **truncated integer** number
    of minutes — while `lastSeen` is reset to `now` unconditionally when the elapsed time is
    positive. So sub-minute activity resets the clock without granting tokens: a client
    polling faster than once a minute never refills. The comment says the opposite ("Only
    update the lastSeen if it has been >= 1 minute ... This allows for continuously sending
    nodes to still get credits"), so the code and the comment disagree. This is a real
    behavior a new backend should decide about deliberately rather than copy.
- `Spend(limitType, bucket, cost, isPriority)`: if `entry.tokens < cost`, return an error;
  else subtract.

### Limits

`pkg/ratelimiter/rate_limiter.go`:

| Constant | Value |
| --- | --- |
| `DEFAULT_RATE_PER_MINUTE` | 4000 |
| `DEFAULT_MAX_TOKENS` | 20000 |
| `PUBLISH_RATE_PER_MINUTE` | 600 |
| `PUBLISH_MAX_TOKENS` | 3000 |
| `DEFAULT_PRIORITY_MULTIPLIER` | 2 |
| `PUBLISH_PRIORITY_MULTIPLIER` | 4 |
| `MAX_UINT_16` | 65535 |

Two limit types: `DEFAULT` (string `"DEF"`) and `PUBLISH` (string `"PUB"`).

A priority client gets `ratePerMinute * multiplier` and `maxTokens * multiplier`, with the
publish multiplier (4) for `PUB` and the default multiplier (2) otherwise
(`pkg/ratelimiter/rate_limiter.go:fillAndReturnEntry`).

### Bucket keys

`pkg/api/interceptor.go:applyLimits`:

```go
ip := utils.ClientIPFromContext(ctx)   // "" -> "ip_unknown"
bucket = ("P" | "R") + ip + string(limitType)
```

- `P` prefix for priority clients, `R` for regular.
- The key is the **client IP plus limit type**, not a wallet or inbox id. Everything without
  an IP shares one `ip_unknown` bucket.
- `pkg/utils/ip.go:ClientIPFromContext` takes the first comma-separated value of the
  `x-forwarded-for` gRPC metadata header, trimmed. If that header is absent it falls back to
  the peer address, split on `:` and taking the first part. (That split is IPv6-unsafe: an
  IPv6 peer address yields only the first hextet. Unverified whether this matters in
  deployment, since a load balancer normally sets `x-forwarded-for`.)

**`x-forwarded-for` is trusted verbatim from the caller.** `ClientIPFromContext` reads the
header straight out of the incoming gRPC metadata and takes its **first** element, with no
check that a trusted proxy set it and no comparison against the real peer address
(`pkg/utils/ip.go:ClientIPFromContext`). The value it returns is what
`pkg/api/interceptor.go:applyLimits` uses for **both** the authz permission lookup and the
rate-limit bucket key. A client that sets its own `x-forwarded-for` therefore chooses which
`ip_addresses` row applies to it — including selecting an `allow_all` or `priority` entry it
does not own — and can mint a fresh, full token bucket per request by varying the value
(a new bucket starts full; see §13, *Bucket lookup detail*). Setting a value that is not in
the table simply yields `Unspecified`, which is also how a `Denied` IP escapes its block.

**The deployment must therefore strip or overwrite `x-forwarded-for` at a trusted upstream.**
This service performs no such sanitization itself: neither the PROXY-protocol listener nor
the gateway removes a caller-supplied value before `applyLimits` reads it. A new backend
should either take the client IP from the transport peer / PROXY-protocol header, or parse
`x-forwarded-for` from the **right-hand** side, discarding as many entries as there are
trusted hops.

### Costs per endpoint

From the `switch req := req.(type)` in `pkg/api/interceptor.go:applyLimits`:

| Request type | Limit type | Cost |
| --- | --- | --- |
| `mlsv1.RegisterInstallationRequest` | `PUB` | 1 |
| `mlsv1.UploadKeyPackageRequest` | `PUB` | 1 |
| `identityv1.PublishIdentityUpdateRequest` | `PUB` | 1 |
| `mlsv1.SendWelcomeMessagesRequest` | `PUB` | `len(req.Messages)` |
| `mlsv1.SendGroupMessagesRequest` | `PUB` | `len(req.Messages)` |
| `messagev1.PublishRequest` (legacy) | `PUB` | `len(req.Envelopes)` |
| `messagev1.BatchQueryRequest` (legacy) | `DEF` | `len(req.Requests)` |
| **everything else** | `DEF` | 1 |

So `BatchPublishCommitLog`, `BatchQueryCommitLog`, `GetNewestGroupMessage`,
`FetchKeyPackages`, `QueryGroupMessages`, `QueryWelcomeMessages`, `GetIdentityUpdates`,
`GetInboxIds`, `VerifySmartContractWalletSignatures`, and all three subscribe methods are
`DEF` cost 1. Streams are charged **once, at open**, in
`pkg/api/interceptor.go:Stream` — a long-lived `Subscribe` costs one token for its whole
lifetime.

The cost is cast `uint16(cost)`, so a batch of 65 536+ would wrap. No handler allows a batch
that large today except `SendGroupMessages`/`SendWelcomeMessages`, which have no cap; the
50 MiB receive limit is the practical bound.

### Error surfaced

| Condition | gRPC code | Message |
| --- | --- | --- |
| Bucket exhausted | `ResourceExhausted` | `<cost> exceeds rate limit <bucket>` — for example `5 exceeds rate limit R203.0.113.7PUB` |

The comment in `Spend` explains the message is deliberately dynamic (it leaks the bucket
name) "for debugging purposes". Because the bucket string contains the derived client IP,
**the rate-limit error echoes the caller's IP back to the caller**.

### Streams are charged once, and always at the default cost

`pkg/api/interceptor.go:Stream` passes the interceptor's `req` argument to `applyLimits`. For
a streaming RPC that argument is the **service implementation** (`srv`), not a request
message, so the `switch req := req.(type)` never matches. Every stream — including a
long-lived XIP-83 `Subscribe` — therefore costs exactly 1 `DEF` token at open, regardless of
how many topics it subscribes or how much data it moves.

### Bucket lookup detail

`pkg/ratelimiter/rate_limiter.go:fillAndReturnEntry` first checks `oldBuckets` with
`createIfMissing = false`, then `newBuckets` with `createIfMissing = true`. A newly created
entry starts **full** (`tokens = limit.MaxTokens * multiplier`, `lastSeen = now`), so a fresh
IP gets a full burst allowance immediately. Because priority and regular use different bucket
prefixes, a permission change also grants a fresh full bucket.

`getLimit(limitType)` falls back to `rl.Limits["default"]` (lower-case literal), but the map
is keyed only by `"DEF"` and `"PUB"`, so an unknown limit type would return nil and nil-deref
in `Refill`. Only `DEFAULT` and `PUBLISH` are ever passed today.

### Bucket lifecycle

`pkg/ratelimiter/rate_limiter.go:Janitor` runs a ticker
(`RATE_LIMITER_SWEEP_INTERVAL = 10 * time.Minute`,
`RATE_LIMITER_EXPIRES_AFTER = 1 * time.Hour`, both in `pkg/api/server.go`) that calls
`sweepAndSwap`: delete entries in `oldBuckets` older than the expiry, emit
`xmtp_ratelimiter_entries_deleted` and `xmtp_ratelimiter_buckets`, then swap `newBuckets`
and `oldBuckets`. This is a two-generation scheme, so an entry survives at most two sweep
intervals of inactivity beyond the expiry check.

---

## 14. Authorization (IP allow list)

`pkg/authz`.

**There is no `--api.authn.allowlists` switch that does anything.** The flag is declared
(`pkg/api/config.go:AuthnOptions.AllowLists`, `long:"allowlists"`) but `s.Authn.AllowLists`
is **never read** outside a test fixture. Two other things control this feature, and both must
be present:

1. `--authz-db-connection-string` — non-empty builds the `DatabaseAllowList` and populates
   `Config.AllowLister` (`pkg/server/server.go`). This is what *creates* the allow list.
2. `--api.authn.ratelimits` — this is what installs the `RateLimitInterceptor`
   (`pkg/api/server.go:startGRPC`), and the interceptor is the **only** caller of
   `GetPermission`. So it is also what *enforces* the allow list.

Neither flag is named "allowlists", and setting `--api.authn.allowlists` alone has no effect
at all. See §17.3 and the two deployment hazards below.

There is **no wallet authentication, no bearer token, and no per-inbox authorization** in
this service. The only authorization is an IP-based allow/deny list, keyed on a client IP the
caller can choose (see §13, *`x-forwarded-for` is trusted verbatim*).

`pkg/authz/permissions.go`:

| Permission | Value | Stored string |
| --- | --- | --- |
| `Unspecified` | 0 | anything unrecognized |
| `AllowAll` | 1 | `allow_all` |
| `Priority` | 2 | `priority` |
| `Denied` | 3 | `denied` |

`pkg/authz/ip_allow_list.go:DatabaseAllowList` loads the `ip_addresses` table
(`pkg/authz/models.go:IPAddress`: `id`, `ip_address`, `created_at`, `deleted_at`,
`permission`, `comment`; migrations in `pkg/migrations/authz`) into an in-memory
`map[string]Permission` and refreshes it every `REFRESH_INTERVAL_SECONDS = 60`.

Enforcement is entirely inside `pkg/api/interceptor.go:applyLimits`, which means **the allow
list only takes effect when rate limiting is enabled**. Behavior:

| Permission for the client IP | Effect |
| --- | --- |
| `Denied` | Request rejected: `PermissionDenied`, message `request blocked`. |
| `AllowAll` | Rate limiting is skipped entirely (`return nil` before any Spend). |
| `Priority` | Bucket prefix `P`, and the priority multipliers apply (2× default, 4× publish). |
| `Unspecified` (the default) | Bucket prefix `R`, base limits. |

Note the code comment: "Debatable whether we want to return a message this clear or
something more ambiguous."

The `--api.authn.enable` flag ("require client authentication via wallet tokens") is marked
`DEPRECATED: This option is no longer used and will be removed in a future release`
(`pkg/api/config.go:AuthnOptions`). `--api.authn.allowlists` is likewise never read, as noted
at the top of this section.

### Refresh mechanism

`pkg/authz/ip_allow_list.go:DatabaseAllowList.start` runs migrations on the authz DB
(`migrate.NewMigrator(d.db, authz.Migrations)`), loads the full permission map once, then
starts `listenForChanges`, a 60 s ticker calling `loadPermissions`.
`loadPermissions` runs `SELECT ... WHERE deleted_at IS NULL`, builds a brand-new map and swaps
it in under a write lock. The comment notes it "loads absolutely everything, since n is going
to be small enough for now". An unrecognized `permission` string logs
`Unknown permission in DB` and is stored as `Unspecified`.

### Two deployment hazards

1. **The deny list is inert without rate limiting.** `GetPermission` is called only from
   `pkg/api/interceptor.go:applyLimits`. With `--authz-db-connection-string` set but
   `--api.authn.ratelimits` unset, denied IPs are served normally.
2. **The reverse combination panics.** With `--api.authn.ratelimits` set but no authz DB,
   `Config.AllowLister` is a nil interface and `applyLimits` dereferences it with no nil
   check on the first request.

---

## 15. Backfillers

Two background processes exist to fill columns added by later migrations. They are relevant
to a new backend only insofar as they explain why `is_commit` and `is_appended` are nullable.

### is_commit backfiller

`pkg/mls/store/backfiller_group_messages.go:IsCommitBackfiller`.

- Loop: `RunInTx` → sqlc `SelectEnvelopesForIsCommitBackfill`
  (`SELECT id, data FROM group_messages WHERE is_commit IS NULL ORDER BY id ASC FOR UPDATE
  SKIP LOCKED LIMIT 100`) → classify via `ValidateGroupMessages` → sqlc
  `UpdateIsCommitStatus` per row.
- `SKIP LOCKED` makes it safe to run on multiple nodes concurrently.
- `maxPayloadSize = 4 * 1024 * 1024` — the batch is split so each validator call stays under
  4 MiB, because "the MLS validation service enforces a maximum request payload size of 4 MiB.
  While the average message is only a few hundred bytes, some can be as large as ~3.5 MiB."
- A single message larger than 4 MiB fails with `InvalidArgument: message too large: <n>
  bytes`.

### is_appended backfiller

`pkg/mls/store/backfiller_installations.go` uses sqlc `SelectInstallationsToBackfill`
(`WHERE is_appended IS NULL ... FOR UPDATE SKIP LOCKED LIMIT 100`) and
`UpdateIsAppendedStatus`. It relates to the `key_packages` table added in
`20250714160210_add_key_packages_migration`.

---

## 16. Pruning / expiry

Pruning is a **separate binary**, `cmd/prune/main.go`, not something the API server does.
`pkg/prune/prune.go:Executor.Run` runs four pruners concurrently.

| Pruner | File | Age constant | What it deletes |
| --- | --- | --- | --- |
| `WelcomePruner` | `pkg/prune/welcomes.go` | `DEFAULT_LIFETIME_OF_WELCOME_MESSAGES = 90` days | `welcome_messages` with `created_at < NOW() - interval` |
| `GroupMessagesPruner` | `pkg/prune/group_messages.go` | `DEFAULT_GROUP_MESSAGE_LIFETIME_DAYS = 30` days | `group_messages` with `is_commit = FALSE` AND `created_at < NOW() - interval` |
| `InstallationsPruner` | `pkg/prune/installations.go` | `DEFAULT_LIFETIME_OF_INSTALLATIONS = 90` days | `installations` older than the cutoff (its `created_at` is a BIGINT of nanoseconds, so the SQL converts: `created_at < (EXTRACT(EPOCH FROM NOW() - (days\|\|' days')::INTERVAL) * 1e9)::BIGINT`) |
| `KeyPackagesPruner` | `pkg/prune/key_packages.go` | `DEFAULT_LIFETIME_OF_KEY_PACKAGES = 90` days | `key_packages` older than the cutoff (same nanosecond conversion) |

Key behaviors:

- **Commits are never pruned.** `DeleteOldGroupMessagesBatch` filters `is_commit = FALSE`, so
  a commit message (and a message whose `is_commit` is still NULL, since `NULL = FALSE` is not
  true) survives forever. This is deliberate: the commit history must remain replayable.
- Deletion is batched with `FOR UPDATE SKIP LOCKED` and `ORDER BY id LIMIT @batch_size`, so it
  is safe to run concurrently and never blocks writers for long.
- The loop repeats until a cycle deletes fewer than `batch_size` rows, or `MaxCycles` is
  reached.
- `batchSize` must be at least 100; the constructors `log.Panic` otherwise.
- Config: `pkg/server/options.go:PruneOptions` / `PruneConfig` — includes `DryRun`,
  `CountDeletable`, `BatchSize`, `MaxCycles`. Count queries *intend* to treat a Postgres
  statement timeout (`SQLSTATE 57014`) as "there is probably work" and return 1 rather than
  failing, but the type check does not match the pgx driver actually in use — see the
  pq-vs-pgx note at the end of this section.
- **Pruning deletes rows but never resets ids**, so cursors remain valid; a client whose
  cursor points into a pruned range simply sees a gap.

Two SQL defects in this area, worth not copying:

- `GetOldInstallations` and `GetOldKeyPackages` compare a BIGINT-nanoseconds `created_at`
  column to a timestamp expression (`NOW() - make_interval(...)`), while the matching delete
  queries correctly convert the cutoff to nanoseconds
  (`pkg/mls/store/queries.sql:GetOldInstallations`, `:GetOldKeyPackages`). These do **not**
  return wrong counts — PostgreSQL has no `bigint < timestamp` operator, so the queries
  **fail** with an operator-does-not-exist error. `Count` then returns that error (it is not
  a `57014` timeout, so the swallow below does not apply),
  `pkg/prune/prune.go:Executor.Run` logs `Error counting envelopes for pruning`, pushes the
  error and **`return`s before pruning that table**. So `--prune.count-deletable` does not
  merely produce a bad number: **it disables the installations and key-packages pruners
  outright** and makes the run report an error. Without the flag both pruners work normally,
  because the delete queries are correct.
- `DeleteOldKeyPackagesBatch`'s CTE selects `installation_id` rather than the primary key
  `sequence_id`, so the `DELETE ... USING` removes **every** row for each matched
  installation, including key packages newer than the cutoff. `batch_size` bounds
  installations, not rows.

Each pruner's `Count` **attempts** to swallow a Postgres statement timeout (`SQLSTATE 57014`)
and return `1, nil` so a slow count does not abort the run, with a comment that "there might
be millions of rows in the DB and a full table scan might take too long".

**That check does not fire on the deployed driver.** The pruners test the error with
`var pqErr *pq.Error; errors.As(err, &pqErr) && pqErr.Code == "57014"`
(`pkg/prune/welcomes.go:Count` and the three siblings) — the **lib/pq** error type. But the
prune binary opens its database through **pgx**: `pkg/server/pgxdb.go:NewPGXDB` builds a
`pgxpool` and wraps it with `stdlib.OpenDBFromPool`. A statement timeout from that stack
arrives as `*pgconn.PgError`, which `errors.As` will not assign to a `*pq.Error`, so the
branch is skipped and `Count` returns `0, err`. `pkg/prune/prune.go:Executor.Run` then logs,
pushes the error, and **aborts that pruner** — the exact outcome the swallow was written to
prevent. Under `--prune.count-deletable`, a count slow enough to hit `statement_timeout`
(set from `--mls-store.read-timeout`, default 10 s) therefore stops the pruner instead of
proceeding. A new backend should match on the driver error type it actually uses, or on the
SQLSTATE string alone.

There is no TTL applied at read time and no expiry field on any message.

A separate in-process cleaner (`pkg/store/cleaner.go`, namespace `store.cleaner`) prunes the
**legacy v1/v2 `message` table** on a live server. It has nothing to do with MLS data.

---

## 17. Server configuration options

Flags are parsed by `github.com/jessevdk/go-flags` from struct tags. `cmd/xmtpd/main.go`
calls `flags.Parse(&options)` against `pkg/server/options.go:Options`. Only these options
affect the v3 MLS/Identity behavior a new backend must replicate; the libp2p/waku options
below are listed for completeness because they configure the legacy path.

### 17.1 Environment variables

There is no general env-var layer. `cmd/xmtpd/main.go:addEnvVars` is a hand-written hook
(the file calls it "a hack") applied **after** flag parsing, so env wins over flags:

| Env var | Overwrites |
| --- | --- |
| `MESSAGE_DB_CONNECTION_STRING` | `Store.DbConnectionString` |
| `MESSAGE_DB_READER_CONNECTION_STRING` | `Store.DbReaderConnectionString` |
| `MLS_DB_CONNECTION_STRING` | `MLSStore.DbConnectionString` |
| `AUTHZ_DB_CONNECTION_STRING` | `Authz.DbConnectionString` |

Three more env vars are read elsewhere:

- `XMTPD_MLS_READER_DB_CONNECTION_STRING` — the only `env:` struct tag in the codebase,
  on `pkg/mls/store/config.go:StoreOptions.DbReaderConnectionString`.
- `ENV` — read in `cmd/xmtpd/main.go` for the DataDog profiler tag (default `"test"`), and
  **independently** in `pkg/api/message/v1/context/context.go:isSupportedClient`, where the
  libxmtp version gate is active only when `ENV == "production"`.
- `GOWAKU-NODEKEY` — hex P2P private key fallback in `pkg/server/server.go:getPrivKey`.
- `MLS_DB_READ_TIMEOUT` — read only by `cmd/prune/main.go:addEnvVars`.

`pkg/server/options.go:ValidateOptions` performs the only validation: `Log.LogEncoding` must
be exactly `"json"` or `"console"`.

### 17.2 API options (namespace `api`) — `pkg/api/config.go:Options`

| Flag | Default | Effect |
| --- | --- | --- |
| `--api.grpc-address` | `0.0.0.0` | gRPC bind address. |
| `--api.grpc-port` | `5556` | gRPC bind port. |
| `--api.http-address` | `0.0.0.0` | grpc-gateway bind address. |
| `--api.http-port` | `5555` | grpc-gateway bind port. |
| `--api.max-msg-size` | `52428800` (50 MiB) | Used twice: `grpc.MaxRecvMsgSize` on the server and `grpc.MaxCallRecvMsgSize` on the gateway's loopback client. **`MaxSendMsgSize` is never set**, so the send limit stays at the gRPC default (`math.MaxInt32`). |
| `--api.enable-mls` | false | Gates registration of `MlsApi` and `IdentityApi`. The gRPC path additionally requires non-nil `MLSStore` **and** non-nil `MLSValidator`; the HTTP gateway path checks only `MLSStore != nil && EnableMLS`. |
| `--api.disable-mls-publish` | false | Passed to both services as `disablePublish`; makes `isPublishDisabled()` always true (§4). |
| `--api.enable-migration` | false | Registers `D14NMigrationApi` and constructs the `CutoverChecker`. **Startup fails** with `d14n-cutover-ns must be specified when migration is enabled` if the cutover is 0. |
| `--api.d14n-cutover-ns` | `0` | Unix nanosecond timestamp after which publishing (and legacy streaming) is refused. A past value logs `d14n-cutover-ns is in the past, publishing is disabled` at boot. Also returned verbatim by `FetchD14NCutover`. |

### 17.3 API authentication options (namespace `api.authn`) — `pkg/api/config.go:AuthnOptions`

| Flag | Default | Effect |
| --- | --- | --- |
| `--api.authn.enable` | false | **Dead.** Marked `DEPRECATED: This option is no longer used`. `s.Authn.Enable` is never read. |
| `--api.authn.ratelimits` | false | The **only** switch that installs the rate-limit interceptor — and therefore the only switch that enforces the authz deny list (§14). |
| `--api.authn.allowlists` | false | **Dead.** `s.Authn.AllowLists` is declared in `pkg/api/config.go:AuthnOptions` but never read outside a test fixture. The allow list is *created* by `--authz-db-connection-string` and *enforced* only by `--api.authn.ratelimits`; this flag contributes nothing (§14). |

**Deployment hazard:** with `--api.authn.ratelimits` set but no `--authz-db-connection-string`,
`Config.AllowLister` is a nil interface and `pkg/api/interceptor.go:applyLimits` calls
`rli.ipAllowList.GetPermission(ip)` with no nil check — the first request panics.

### 17.4 MLS store options (namespace `mls-store`) — `pkg/mls/store/config.go`

| Flag | Default | Effect |
| --- | --- | --- |
| `--mls-store.db-connection-string` | `""` | Master switch for MLS. Non-empty → builds the writer store (`mlsstore.New`) and the read store (`mlsstore.NewReadStore`). |
| `--mls-store.reader-db-connection-string` | `""` | Read-replica DSN. Empty → logs `no reader db connection string provided. Using the same db for reads and writes` and falls back to the writer DSN. |
| `--mls-store.read-timeout` | `10s` | Passed to `NewPGXDB` as `statementTimeout`; it sets the Postgres `statement_timeout` runtime parameter in milliseconds, **not** a driver read deadline. |
| `--mls-store.write-timeout` | `10s` | Used only by `CreateMlsMigration`; the pgx pool ignores it. |
| `--mls-store.max-open-conns` | `80` | Declared but **not applied** on the pgx pool path (`NewPGXDB`). |

### 17.5 MLS validation options (namespace `mls-validation`) — `pkg/mlsvalidate/config.go`

| Flag | Default | Effect |
| --- | --- | --- |
| `--mls-validation.grpc-address` | `""` | Address of the external validation service. Empty → nil `MLSValidator` → the MLS and Identity gRPC services are not registered, and the is_commit backfiller does not run. Dialed insecurely (plaintext). |

### 17.6 Authz options (no namespace) — `pkg/server/options.go:AuthzOptions`

| Flag | Default | Effect |
| --- | --- | --- |
| `--authz-db-connection-string` | `""` | Non-empty → builds `authz.NewDatabaseAllowList`, which self-migrates the authz DB on boot and refreshes the IP permission map every 60 s. |
| `--authz-read-timeout` | `10s` | `pgdriver.WithReadTimeout` for the authz DB. |
| `--authz-write-timeout` | `10s` | `pgdriver.WithWriteTimeout`. |

The authz pool's `MaxOpenConns` is taken from `--store.max-open-conns`.

### 17.7 Legacy message store options (namespace `store`) — `pkg/store/config.go`

Relevant to a v3 backend only because `pkg/api/config.go:Config.validate` returns
`ErrMissingStore` when `Store` is nil, so **the API server cannot start without
`--store.enable`** even in an MLS-only deployment.

| Flag | Default | Effect |
| --- | --- | --- |
| `--store.enable` | false | Required for the API server to start at all. |
| `--store.db-connection-string` | `""` | v1/v2 message DB writer DSN. |
| `--store.reader-db-connection-string` | `""` | Reader DSN. No fallback to the writer DSN (unlike the MLS store). |
| `--store.db-read-timeout` | `10s` | Driver read timeout. |
| `--store.db-write-timeout` | `10s` | Driver write timeout. |
| `--store.max-open-conns` | `80` | Applied to all four message-store connections **and** the authz DB. |
| `--store.metrics-period` | `30s` | Ticker for `xmtp_stored_messages`. Skipped when 0. |

Message-DB cleaner (namespace `store.cleaner`, `pkg/store/cleaner.go`) — the v1/v2 cleaner,
distinct from `cmd/prune`: `--store.cleaner.enable` (false),
`--store.cleaner.active-period` (`5s`), `--store.cleaner.passive-period` (`5m`),
`--store.cleaner.retention-days` (`1`), `--store.cleaner.batch-size` (`50000`),
`--store.cleaner.read-timeout` (`60s`), `--store.cleaner.write-timeout` (`60s`).

### 17.8 Logging, metrics, tracing, profiling

| Flag | Default | Effect |
| --- | --- | --- |
| `--log-level` / `-l` | `INFO` | zap level. **Toggled at runtime by SIGUSR1**, which flips between Debug and Info (`cmd/xmtpd/main.go`). |
| `--log-encoding` | `console` | `console` or `json`; the only validated option. |
| `--metrics` | false | Starts the Prometheus HTTP server and the peer-status loop. |
| `--metrics-address` | `127.0.0.1` | Prometheus bind address. |
| `--metrics-port` | `8008` | Prometheus bind port. The handler serves on **all paths**, not just `/metrics`. |
| `--metrics-period` | `30s` | Peer-status gauge loop period (distinct from `--store.metrics-period`). |
| `--tracing` | false | DataDog APM tracing. |
| `--profiling.enable` | false | DataDog continuous profiler (CPU + heap always). |
| `--profiling.block` | false | Adds block profiling ("more overhead"). |
| `--profiling.mutex` | false | Adds mutex profiling. |
| `--profiling.goroutine` | false | Adds goroutine profiling. |
| `--go-profiling` | false | Starts `net/http/pprof` on `0.0.0.0:6060`. |

The Prometheus registry is always created and always passed to waku, regardless of
`--metrics`; only the HTTP server is gated.

### 17.9 Operational one-shot flags

| Flag | Effect |
| --- | --- |
| `--version` | Prints `Version: <Commit>` (link-time var) and exits. |
| `--generate-key` | Writes a P2P private key to `--key-file` (mode `0o600`) and exits. `--overwrite` permits replacing an existing file, otherwise it errors `<file> already exists. Use --overwrite to overwrite the file`. |
| `--create-message-migration <name>` | Creates a Bun migration file for the messages DB and exits. |
| `--create-authz-migration <name>` | Same for authz. |
| `--create-mls-migration <name>` | Same for MLS. Its log line is a copy-paste bug reading `created authz migration ...`. |
| `--wait-for-db` (`30s`) | Retry window when opening the databases at boot. **Two different implementations, see below.** |

`--wait-for-db` feeds two unrelated startup paths, and a new backend should not assume one
behavior:

| Path | Databases | Mechanism | Failure text |
| --- | --- | --- | --- |
| `pkg/server/server.go:createDB` (bun + `pgdriver`) | legacy message store, authz | Loop: `db.Ping`, `time.Sleep(3 * time.Second)`, retry until `time.Now().Add(waitForDB)` passes | `timeout waiting for db` |
| `pkg/server/pgxdb.go:WaitUntilDBReady` (pgx pool) | MLS store (`newBunPGXDb`/`NewPGXDB`), and the whole `cmd/prune` binary | **One** `dbpool.Ping` under a `context.WithTimeout(ctx, waitTime)` — no 3 s retry loop | `database is not ready within <d>: <err>` |

So the three-second polling and the `timeout waiting for db` message apply only to the
legacy/authz driver path. The MLS and prune paths make a single deadline-bounded ping and
report different text.

### 17.10 libp2p / waku options (legacy path)

`--port`/`-p` (60000), `--address` (0.0.0.0), `--ws`, `--ws-port` (60001),
`--ws-address` (0.0.0.0), `--nodekey`, `--key-file` (`./nodekey`), `--static-node`
(repeatable; re-dialed every 500 ms), `--keep-alive` (20 s libp2p ping interval, unrelated to
gRPC keepalive), `--no-relay`, `--topics` (defaults to the waku default topic),
`--min-relay-peers-to-publish` (0).

### 17.11 Prune binary options (`cmd/prune`) — `pkg/server/options.go:PruneConfig`

| Flag | Default | Effect |
| --- | --- | --- |
| `--prune.max-prune-cycles` | `10` | Batch iterations per pruner before giving up (`Reached maximum pruning cycles`). Must be ≥ 1. |
| `--prune.batch-size` | `10000` | Rows per delete batch. Pruner constructors **panic** if < 100. Also the loop-exit test: a cycle deleting fewer than this ends the pruner. |
| `--prune.count-deletable` | false | Runs the count query first and skips the pruner if it returns 0. |
| `--prune.dry-run` | false | Logs and returns before any delete. |

The prune binary also accepts `--log-level`, `--log-encoding`, `--version`, and the
`--mls-store.*` group. It requires `--mls-store.db-connection-string`
(`missing required arguments: --mls-store.db-connection-string`). Its DB wait is a hardcoded
30 s and its `--mls-store.read-timeout` becomes the Postgres `statement_timeout`.
**Retention windows are compile-time constants and are not configurable.**

### 17.12 Shutdown behavior

`pkg/api/server.go:Close` cancels the context, unsubscribes the waku relay subscription,
closes `messagev1`, `mlsv1` and `migrationv1`, closes the HTTP then the gRPC listener, and
waits on the WaitGroup. Notable gaps for a new implementation to consider:

- **`grpcServer.GracefulStop()` is never called** — the listener is simply closed, so
  in-flight RPCs are dropped abruptly rather than drained.
- `identityv1.Close()` is never called.
- `authz.DatabaseAllowList.Stop()` is never called; its refresh goroutine unwinds only via
  the parent context.
- The health server's serving status is never set to `NOT_SERVING`, so a draining node keeps
  reporting `SERVING`.

---

## 18. Metrics

Names as declared in `pkg/metrics/*.go`.

| Metric | Source file | Note |
| --- | --- | --- |
| `xmtp_ratelimiter_buckets` | `pkg/metrics/ratelimiter.go` | Gauge of bucket-map size per generation. |
| `xmtp_ratelimiter_entries_deleted` | `pkg/metrics/ratelimiter.go` | Entries swept by the janitor. |
| `xmtp_subscribed_topics` | `pkg/metrics/subscriptions.go` | Gauge of live-subscribed topics. |
| `xmtp_subscribe_topics_length` | `pkg/metrics/subscriptions.go` | Histogram of topics per subscribe call. |
| `xmtp_api_requests` | `pkg/metrics/api.go` | API request counter. |
| `xmtp_api_request_duration_ms` | `pkg/metrics/api.go` | Request duration histogram. |
| `xmtp_published_envelope` | `pkg/metrics/api.go` | Published envelope size histogram (legacy path). |
| `xmtp_published_envelopes` | `pkg/metrics/api.go` | Published envelope counter (legacy path). |
| `xmtp_api_query_duration` | `pkg/metrics/api.go` | Query duration. |
| `xmtp_api_query_result` | `pkg/metrics/api.go` | Query result size. |
| `mls_sent_group_message_size` | `pkg/metrics/mls.go` | Histogram, emitted by `SendGroupMessages`. |
| `mls_sent_group_messages` | `pkg/metrics/mls.go` | Counter. |
| `mls_sent_welcome_message_size` | `pkg/metrics/mls.go` | Histogram, emitted by `SendWelcomeMessages`. |
| `mls_sent_welcome_messages` | `pkg/metrics/mls.go` | Counter. |
| `mls_commit_log_entry_size` | `pkg/metrics/mls.go` | Histogram, emitted by `BatchPublishCommitLog`. |
| `mls_commit_log_entry_count` | `pkg/metrics/mls.go` | Counter. |
| `mls_sent_identity_update_size` | `pkg/metrics/mls.go` | Histogram, emitted by `PublishIdentityUpdate`. |
| `mls_sent_identity_updates` | `pkg/metrics/mls.go` | Counter. |
| `mls_sent_key_package_size` | `pkg/metrics/mls.go` | Histogram, emitted by RegisterInstallation / UploadKeyPackage. |
| `mls_sent_key_packages` | `pkg/metrics/mls.go` | Counter. |
| `xmtp_peers_by_proto` | `pkg/metrics/peers.go` | libp2p; legacy. |
| `xmtp_bootstrap_peers` | `pkg/metrics/peers.go` | libp2p; legacy. |
| `xmtp_stored_messages` | `pkg/metrics/store.go` | Legacy message store. |

In addition, `go-grpc-prometheus` is registered (`prometheus.Register(grpcServer)` in
`pkg/api/server.go:startGRPC`) with `EnableHandlingTimeHistogram()`, giving the standard
`grpc_server_started_total`, `grpc_server_handled_total`,
`grpc_server_msg_received_total`, `grpc_server_msg_sent_total` and
`grpc_server_handling_seconds` families.

`metrics.EmitSubscribeTopics` / `EmitUnsubscribeTopics` are called by the XIP-83 handler on
every add that becomes live and every drop, plus a teardown defer that unsubscribes the
remaining count (`pkg/mls/api/v1/subscribe.go:Subscribe`, `dropTopic`).

---

## 19. Summary of limits and constants

| Limit | Value | Where | Applies to |
| --- | --- | --- | --- |
| Max gRPC receive size | 52428800 (50 MiB) | `pkg/api/config.go:Options.MaxMsgSize` (`--api.max-msg-size`) | every RPC |
| Max page size | 100 | `pkg/mls/store/store.go:maxPageSize` | QueryGroupMessages, QueryWelcomeMessages, QueryCommitLog |
| Default page size | 100 | same | same |
| Max batch inserts | 10 | `pkg/mls/api/v1/service.go:maxBatchInserts` | BatchPublishCommitLog |
| Max batch queries | 20 | `pkg/mls/api/v1/service.go:maxBatchQueries` | BatchQueryCommitLog |
| Max inbox log entries | 256 | `pkg/mls/store/store.go:PublishIdentityUpdate` | PublishIdentityUpdate |
| Max adds per Mutate | 100000 | `pkg/mls/api/v1/subscribe.go:maxMutateAdds` | XIP-83 Subscribe |
| Ping interval / keepalive | 30 s | `pkg/mls/api/v1/subscribe.go:subscribePingInterval` | XIP-83 Subscribe |
| Pong deadline | 30 s | `pkg/mls/api/v1/subscribe.go:subscribePongDeadline` | XIP-83 Subscribe |
| Server frame target | 2 MiB | `maxFrameBytes` | XIP-83 Subscribe |
| Gated live buffer | 64 MiB | `maxPendingBytes` | XIP-83 Subscribe |
| Catch-up fetch budget | 64 MiB | `catchUpMaxPendingBytes` | XIP-83 Subscribe |
| Rows per topic per scan turn | 64 | `catchUpTopicPageLimit` | XIP-83 Subscribe |
| Topics per scan turn | 256 | `catchUpBatchTopics` | XIP-83 Subscribe |
| Concurrent scans per stream | 4 | `catchUpMaxConcurrentScans` | XIP-83 Subscribe |
| Live channel depth (XIP-83) | 4096 | `subscribeBacklog` | XIP-83 Subscribe |
| Live channel depth (legacy subscribe) | 1024 | `pkg/subscriptions/dispatcher.go:minBacklogBufferLength` | SubscribeGroupMessages, SubscribeWelcomeMessages |
| All-topics channel depth | 4096 | `allTopicsBacklogLength` | legacy SubscribeAll |
| Live poll interval | 25 ms | `pkg/mls/api/v1/worker.go:DEFAULT_POLL_INTERVAL` | all live delivery |
| Live poll batch | 500 rows | `pkg/mls/api/v1/worker.go` `Numrows: 500` | all live delivery |
| Validator payload cap | 4 MiB | `pkg/mls/store/backfiller_group_messages.go:maxPayloadSize` | **is_commit backfiller only.** Not applied by `SendGroupMessages`, which sends the whole batch unsplit and unmeasured. The validation service's own limit is external and unverified here (§8.1). |
| Default rate | 4000/min, max 20000 | `pkg/ratelimiter/rate_limiter.go` | `DEF` bucket |
| Publish rate | 600/min, max 3000 | same | `PUB` bucket |
| Priority multipliers | 2× default, 4× publish | same | priority IPs |
| Rate limiter sweep | every 10 min, expire after 1 h | `pkg/api/server.go` | all buckets |
| Allow list refresh | 60 s | `pkg/authz/ip_allow_list.go:REFRESH_INTERVAL_SECONDS` | IP permissions |
| Group message retention | 30 days (non-commit only) | `pkg/prune/group_messages.go` | pruner |
| Welcome retention | 90 days | `pkg/prune/welcomes.go` | pruner |
| Installation retention | 90 days | `pkg/prune/installations.go` | pruner |
| Key package retention | 90 days | `pkg/prune/key_packages.go` | pruner |
| gRPC server keepalive `Time` | 5 min | `pkg/api/server.go:startGRPC` | transport |
| gRPC keepalive enforcement `MinTime` | 15 s, `PermitWithoutStream: true` | same | transport |
| PROXY protocol header timeout | 10 s | same | transport |
| Cutover streaming re-check | 5 s | `pkg/mls/api/v1/service.go` cutoverTicker | legacy subscribe only |

---

## 20. Open questions / unverified items

1. **`Subscribe` (XIP-83) does not check the D14N cutover.** The two legacy subscribe methods
   do. Unverified whether this is intentional or an omission.
2. **`UploadKeyPackage` indexes `validationResults[0]` without a length check**
   (`pkg/mls/api/v1/service.go:UploadKeyPackage`), unlike `RegisterInstallation`. A validator
   response with zero entries and no error would panic. Unverified whether the validator can
   produce that.
3. **The rate limiter's refill comment contradicts its code** (§13). Unverified which was
   intended.
4. **`PublishIdentityUpdate` uses REPEATABLE READ, but its doc comment says SERIALIZABLE —
   and the combination does not prove the cumulative-validation property.** The advisory lock
   serializes *execution* per inbox, but it does not fix the transaction's *snapshot*. Under
   REPEATABLE READ the snapshot is established by the transaction's first statement, and
   `LockInboxLog` is itself that first statement (`pkg/mls/store/store.go:PublishIdentityUpdate`,
   `pkg/mls/store/queries.sql:LockInboxLog`). A publisher can take its snapshot **while
   blocked waiting for the lock**, before the prior holder commits; when the lock is released
   the `GetAllInboxLogs` read then runs against that stale snapshot and misses the update the
   previous publisher just wrote. The new update is therefore validated against an incomplete
   log even though the two transactions were serialized. The sequence id assignment is still
   correctly ordered (that is the function's own lock), so this affects validation
   cumulativeness, not ordering. A new backend should read the log at a snapshot taken
   **after** acquiring the lock, or use SERIALIZABLE and retry, rather than assume the lock
   is sufficient. The 3-attempt retry in `RunInRepeatableReadTx` does not address this,
   because no serialization failure is raised.
5. **`GetInboxIds` ignores `identifier_kind`** and never populates it on the response.
   Unverified whether any client depends on the echo.
6. **The dispatcher's `log2` buffer sizing is dead code** (always clamped to 1024).
7. **`ClientIPFromContext` splits the peer address on `:`**, which mangles IPv6. Unverified
   whether any deployment reaches that fallback path.
8. **`SendGroupMessages` and `SendWelcomeMessages` have no batch-size cap.** The only bound
   this repository applies is the 50 MiB receive limit. The 4 MiB figure is the *backfiller's*
   constant and is not enforced on the send path (§8.1); whatever cap the validation service
   applies is external and unverified here. Unverified what real clients send.
   `SendGroupMessages` additionally never checks that the validator returned one result per
   message (§8.1), so a short result slice silently publishes a prefix and a long one panics.
9. **The id-visibility-order invariant** the XIP-83 ceiling relies on is asserted in comments
   as "a pre-existing v3 property", not enforced by any mechanism the code owns. A new backend
   should provide it explicitly.
10. **The `commit_log` (v1) table and `insert_commit_log` function** still exist alongside
    `commit_log_v2`; migration `20250730004640_modify_commit_log` carries a TODO to drop them
    "once servers have been deployed". Only v2 is used by the API. Note both v1 and v2 share
    the advisory-lock namespace `'commit_log_sequence'`, so their inserts serialize against
    each other for the same group.
11. **`GetOldInstallations` / `GetOldKeyPackages` compare nanoseconds to a timestamp** (§16).
    Resolved: the comparison has no PostgreSQL operator, so the queries **error** rather than
    return a wrong number, and the error aborts those two pruners. `--prune.count-deletable`
    is therefore broken for installations and key packages. The corresponding deletes are
    correct, so both pruners work when the flag is off.
12. **`DeleteOldKeyPackagesBatch` deletes by `installation_id`, not by row** (§16), so it can
    remove key packages newer than the retention cutoff. Unverified whether this is
    intentional (an installation-level purge) or a bug.
13. **`pkg/api/message/v1/context/context.go:NewRequesterInfo` calls
    `md.Append("X-User-Id", "real_user_id")`** — a hardcoded literal appended to incoming
    metadata on every request. Nothing reads it. It appears to be leftover scaffolding.
14. **`grpcServer.GracefulStop()` is never called**, `identityv1` is never closed, the authz
    allow list is never stopped, and the health server never reports `NOT_SERVING` (§17.12).
    A new backend should drain deliberately.
15. **The API server refuses to start without `--store.enable`** even in an MLS-only
    deployment, because `pkg/api/config.go:Config.validate` returns `ErrMissingStore`.
16. **`incomingHeaderMatcher` allows only three headers through the HTTP gateway**
    (`x-app-version`, `x-client-version`, `x-libxmtp-version`) and returns false for
    everything else, `x-forwarded-for` included. IP-based limiting still works over HTTP
    because grpc-gateway's `runtime.AnnotateContext` handles `x-forwarded-for` itself,
    independently of the matcher. Unverified against the exact grpc-gateway version vendored
    here.
17. **`gzipWrapper` compresses only paths ending in `/query` or `/batch-query`**
    (`pkg/api/gzip_handler.go`), deliberately excluding streaming endpoints because "clients
    still accept gzip encoding for /subscribe which is problematic".
18. **The libxmtp version gate compares the whole header value, not a parsed version** (§4),
    so the conventional `libxmtp/1.1.4` form fails the semver check and is allowed through.
    Unverified whether real clients send a bare version and are therefore actually gated, or
    whether the gate is effectively inert in production.
19. **`GetIdentityUpdates` result grouping is case-sensitive on `inbox_id`** (§10.2): SQL
    canonicalizes to lowercase hex, Go looks up the raw request string, so an uppercase-hex
    request silently returns an empty update list. Unverified whether any client sends
    uppercase.
20. **Repeated `inbox_id` filters in one `GetIdentityUpdates` request are merged, not
    evaluated per slot** (§10.2). Unverified whether any client batches the same inbox twice.
21. **`SubscribeRequest.v1` with an unrecognized inner arm is silently dropped** (§9.18) —
    no response, no error — while a missing outer `v1` fails fast. Unverified whether this
    asymmetry is intentional.
22. **`x-forwarded-for` is taken from the caller with no trusted-proxy check** (§13), so the
    client chooses its own authz identity and rate-limit bucket. Unverified what the
    production load balancer does with a caller-supplied header, which is what determines
    whether this is exploitable in deployment.
23. **The prune count-timeout swallow matches `*pq.Error` while the binary runs on pgx**
    (§16), so a timed-out count aborts the pruner instead of proceeding. Unverified whether
    counts ever reach `statement_timeout` at production data volumes.
24. **The advisory lock keys are 32-bit `hashtext` values** (§6), so unrelated topics can
    share a lock. Unverified what collision rate the production topic set produces; the
    effect is reduced write concurrency, not incorrect ordering.

---

## Review status

This page was checked by an adversarial review (Codex, `gpt-5.6-sol`, read-only sandbox,
`model_reasoning_effort: high`) against the source repository, and the findings below were
then re-verified against the code before the text was changed.

**Review thread id:** `01a06248-4d20-7330-8c4c-83bb62fb9cec`
**Run log:** `/Users/nickmolnar/.claude/jobs/55a23e1f/tmp/phase0/runs/review-wiki-node-go.md`
**Verdict:** ISSUES — 22 findings, all confirmed correct against the code and applied.

| # | Finding | Status | Note |
| --- | --- | --- | --- |
| 1 | §10.2: a duplicate `inbox_id` does **not** produce independent results — the join multiplies rows and Go merges them by inbox id | applied | §10.2 rewritten: equal filters duplicate updates, different cursors yield one combined list in both slots. `pkg/mls/store/queries.sql:GetInboxLogFiltered`, `pkg/mls/store/readStore.go:GetInboxLogs`. |
| 2 | §4: the version gate compares the **whole** header value, not a parsed `name/version` | applied | §4 rewritten with the `parseVersionHeaderValue` third-return detail and the `libxmtp/1.1.4` fail-open consequence. `pkg/api/message/v1/context/context.go:NewRequesterInfo`, `:isSupportedClient`. |
| 3 | §10.2: `GetIdentityUpdates` reads the **primary**, not the replica | applied | §10.2 corrected; `Store.New` builds its internal read store on the writer `db`. `pkg/mls/store/store.go:New`, `pkg/identity/api/v1/identity_service.go:GetIdentityUpdates`. |
| 4 | §10.1: validator errors are **not** flattened to `Unknown` — the gRPC status passes through | applied | Error table row changed and a passthrough paragraph added tracing the four return hops. `pkg/mlsvalidate/service.go:GetAssociationState`, `pkg/mls/store/transactions.go:RunInTx`. |
| 5 | §8.1: no check that `len(validationResults) == len(req.Messages)` | applied | Validation step 3 expanded; two rows added to the error table (silent prefix publish; index-out-of-range panic). `pkg/mls/api/v1/service.go:SendGroupMessages`. |
| 6 | §§8.8, 8.9, 8.13, 10.2: cursor-overflow behavior differs per path | applied | Four-row comparison table added to §8.8, with cross-references from §8.9 and §8.13. `pkg/mls/store/readStore.go` (`QueryGroupMessagesV1`, `QueryWelcomeMessagesV1`, `QueryCommitLog`, `GetInboxLogs`, `clampCursor`). |
| 7 | §10.2: inbox-id grouping is case-sensitive after SQL lowercases it | applied | New paragraph in §10.2: uppercase hex matches in SQL, returns an empty `updates` list. `pkg/mls/store/queries.sql:GetInboxLogFiltered`, `pkg/mls/store/readStore.go:GetInboxLogs`. |
| 8 | §20 item 4: REPEATABLE READ + advisory lock does not prove cumulative validation | applied | Item 4 rewritten: the snapshot can be taken while blocked on the lock, so the log read can be stale. `pkg/mls/store/store.go:PublishIdentityUpdate`, `pkg/mls/store/queries.sql:LockInboxLog`. |
| 9 | §8.1, §19: the 4 MiB cap is the backfiller's, not a send-path limit | applied | §8.1 Limits rewritten and the §19 row re-scoped. `pkg/mls/store/backfiller_group_messages.go:maxPayloadSize`, `pkg/mls/api/v1/service.go:SendGroupMessages`. |
| 10 | §9.18: a `v1` with no recognized inner arm is silently ignored | applied | New paragraph after the error table; the inner `switch` has no `default`. `pkg/mls/api/v1/subscribe.go:Subscribe`. |
| 11 | §§9.6, 9.12: no cross-kind total order — group and welcome ids are separate sequences | applied | §9.6 item 5 and §9.12 step 2 corrected to "ascending within each kind". `pkg/mls/api/v1/subscribe.go:completeWave`. |
| 12 | §9.16: `lastActivity` records queue admission, not `stream.Send` success | applied | §9.16 bullet rewritten with the `case outbound <- flat:` detail and the `sendQueueDepth = 8` consequence. `pkg/mls/api/v1/subscribe.go`. |
| 13 | §§8.1, 12: 25 ms is the poll cadence, not a latency floor | applied | Corrected in both places to a ~0–25 ms added wait. `pkg/mls/api/v1/worker.go:DEFAULT_POLL_INTERVAL`. |
| 14 | §16: the installation/key-package count queries **fail**, they do not return meaningless counts | applied | §16 defect bullet and §20 item 11 rewritten: no `bigint < timestamp` operator, so the error aborts those pruners under `--prune.count-deletable`. `pkg/mls/store/queries.sql`, `pkg/prune/prune.go:Executor.Run`. |
| 15 | §16: the `57014` swallow tests `*pq.Error` while the binary runs on pgx | applied | New paragraph at the end of §16, plus the earlier config bullet softened. `pkg/prune/welcomes.go:Count`, `pkg/server/pgxdb.go:NewPGXDB`. |
| 16 | §14: `--api.authn.allowlists` is never read | applied | §14 opens with the two flags that actually matter; §17.3 row expanded; the later duplicate sentence made consistent. `pkg/api/config.go:AuthnOptions`, `pkg/api/server.go:startGRPC`. |
| 17 | §§13–14: the first caller-supplied `x-forwarded-for` value is trusted | applied | New subsection in §13 and a pointer from §14. `pkg/utils/ip.go:ClientIPFromContext`, `pkg/api/interceptor.go:applyLimits`. |
| 18 | §5.6: 43 named queries, not 40 | applied | Count corrected; the table already listed 43. `pkg/mls/store/queries.sql`. |
| 19 | §§1, 11: the wire names are `D14nMigrationApi` / `FetchD14nCutover` | applied | §1 and §11 corrected, with the Go-vs-wire casing split spelled out. `pkg/proto/migration/api/v1/migration_grpc.pb.go`. |
| 20 | §17.9: `--wait-for-db` has two implementations with different behavior and error text | applied | §17.9 row replaced by a two-path table. `pkg/server/server.go:createDB`, `pkg/server/pgxdb.go:WaitUntilDBReady`. |
| 21 | §6: `hashtext` is 32-bit, so lock keys can collide | applied | New consequence 5 in §6. `pkg/migrations/mls/20250710145538_add-sender-hmac.up.sql`, `pkg/migrations/mls/20240829001344_serial-ids.up.sql`. |
| 22 | No MLS or Identity RPC is omitted; no missing final-schema column or index | applied (confirmation) | No change needed. The review independently confirmed 15 MLS methods and 4 Identity methods are all covered. |

Seven new entries (§20 items 18–24) were added to the open-questions list for the findings
that raise a question the code alone does not settle.

**No finding was rejected.** Every one was reproduced in the source before the text was
changed.

### Residual risk

The corrections above were each verified by reading the cited function, but the review
sampled rather than exhausted the page, so the parts it did not reach carry unchanged
confidence. Specifically **not** re-verified in this pass: the full §5.2 column-by-column
schema tables and the §5.5 sqlc model field list (the review confirmed the migration files
and reported no missing final-schema column or index, but the individual Go types and
nullability flags were not re-read); the §5.6 per-query descriptions beyond the ones named
in a finding; the §8.2–§8.7 and §8.10–§8.14 error tables, whose message strings were spot-
checked by the review but not re-derived line by line here; the §9.1–§9.15 and §9.17 XIP-83
narrative other than the four points a finding touched, including the exact constant values
in §9.19; the §17 flag defaults outside §§17.3 and 17.9; and every §18 metric name. Three
classes of claim are inherently weaker than a code citation and remain so: statements about
the **external** validation service's behavior (its 4 MiB cap, its status codes, whether it
can return a mismatched result count), which live outside this repository; statements about
what the **production deployment** does (whether a load balancer strips `x-forwarded-for`,
whether `ENV=production` is set, whether counts reach `statement_timeout`), which no source
file settles; and the runtime consequences asserted for defects that were reasoned about
rather than executed — notably the panic on a long validator result slice and the pq/pgx
timeout mismatch, neither of which was reproduced against a live database. The page reflects
the repository at commit `7561deb64aa5a324c13d68c8234bf7b4d7fcde9d`; later commits are not
covered.
