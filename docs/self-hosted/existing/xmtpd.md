<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# xmtpd — Current Behavior Reference (XMTP v4 backend)

**Repository**: `/Users/nickmolnar/code/xmtp/xmtpd`
**Commit**: `822ddc956a4408c8465d911bdf0e44fc2100bd7c` — "feat(api): XIP-83 Subscribe — tag deliveries
with wave mutate_id and enforce ordering guarantees (#2035)"
**Language**: Go. RPC framework: **connect-go** (`connectrpc.com/connect`) serving gRPC + gRPC-Web over
HTTP/2 (h2c). Database: **PostgreSQL** with `sqlc`-generated queries and `golang-migrate` migrations.

This document describes what xmtpd does **today**. It is written for an implementer building a
replacement: a self-hosted Rust backend with a single Postgres database, **no blockchain, no payer
service, no originators**. Sections marked **[DROP]** describe machinery that exists only to serve the
decentralized/blockchain model. Sections marked **[KEEP]** describe behavior worth preserving.

Every claim cites a file path and a function, type, or SQL query name. Items I could not verify are
marked **[UNVERIFIED]**.

---

## Table of contents

1. [Overview and process model](#1-overview-and-process-model)
2. [The envelope model](#2-the-envelope-model)
   - [2.1 Layer diagram](#21-layer-diagram)
   - [2.2 ClientEnvelope](#22-clientenvelope)
   - [2.3 AuthenticatedData (AAD)](#23-authenticateddata-aad)
   - [2.4 PayerEnvelope](#24-payerenvelope)
   - [2.5 UnsignedOriginatorEnvelope](#25-unsignedoriginatorenvelope)
   - [2.6 OriginatorEnvelope](#26-originatorenvelope)
   - [2.7 Cursor](#27-cursor)
   - [2.8 Signature scheme summary](#28-signature-scheme-summary)
   - [2.9 What a single-Postgres backend does not need](#29-what-a-single-postgres-backend-does-not-need)
3. [Topics](#3-topics)
4. [Retention and expiry](#4-retention-and-expiry)
5. [gRPC service surface](#5-grpc-service-surface)
6. [Endpoint reference](#6-endpoint-reference)
   - [6.0 IMPORTANT — the wire message is not the handler message](#60-important--the-wire-message-is-not-the-handler-message)
   - [6.1 ReplicationApi.PublishPayerEnvelopes](#61-replicationapipublishpayerenvelopes)
   - [6.2 ReplicationApi/QueryApi.QueryEnvelopes](#62-replicationapiqueryapiqueryenvelopes)
   - [6.3 QueryApi.Subscribe (XIP-83)](#63-queryapisubscribe-xip-83)
   - [6.4 ReplicationApi/QueryApi.SubscribeTopics](#64-replicationapiqueryapisubscribetopics)
   - [6.5 ReplicationApi.SubscribeEnvelopes](#65-replicationapisubscribeenvelopes)
   - [6.6 ReplicationApi.SubscribeOriginators](#66-replicationapisubscribeoriginators)
   - [6.7 NotificationApi.SubscribeAllEnvelopes](#67-notificationapisubscribeallenvelopes)
   - [6.8 ReplicationApi/QueryApi.GetNewestEnvelope](#68-replicationapiqueryapigetnewestenvelope)
   - [6.9 ReplicationApi/QueryApi.GetInboxIds](#69-replicationapiqueryapigetinboxids)
   - [6.10 PublishApi.PublishPayerEnvelopes](#610-publishapipublishpayerenvelopes)
   - [6.11 PayerApi/GatewayApi.PublishClientEnvelopes](#611-payerapigatewayapipublishclientenvelopes)
   - [6.12 PayerApi/GatewayApi.GetNodes](#612-payerapigatewayapigetnodes)
   - [6.13 MetadataApi.GetSyncCursor](#613-metadataapigetsynccursor)
   - [6.14 MetadataApi.SubscribeSyncCursor](#614-metadataapisubscribesynccursor)
   - [6.15 MetadataApi.GetVersion](#615-metadataapigetversion)
   - [6.16 MetadataApi.GetPayerInfo](#616-metadataapigetpayerinfo)
   - [6.17 MisbehaviorApi (not served)](#617-misbehaviorapi-not-served)
7. [Ordering and sequence ids](#7-ordering-and-sequence-ids)
8. [Database schema](#8-database-schema)
9. [Partitioning and the advisory lock](#9-partitioning-and-the-advisory-lock)
10. [Identity updates end to end](#10-identity-updates-end-to-end)
11. [MLS validation service](#11-mls-validation-service)
12. [Subscriptions: the live delivery machine](#12-subscriptions-the-live-delivery-machine)
13. [Rate limiting](#13-rate-limiting)
14. [Authentication and interceptors](#14-authentication-and-interceptors)
15. [Pruning and expiry enforcement](#15-pruning-and-expiry-enforcement)
16. [Configuration](#16-configuration)
17. [Metrics](#17-metrics)
18. [Limits, one table](#18-limits-one-table)
19. [Recommendations for the new backend](#19-recommendations-for-the-new-backend)

---

## 1. Overview and process model

xmtpd is one Go binary (`cmd/replication/main.go`) that runs several cooperating subsystems in one
process, all sharing one Postgres database:

| Subsystem | Entry point | What it does |
| --- | --- | --- |
| API server | `pkg/api/server.go`, `NewAPIServer` | HTTP/2 (h2c) mux serving ReplicationApi, QueryApi, PublishApi, NotificationApi, MetadataApi |
| Publish worker | `pkg/api/message/publish_worker.go`, `startPublishWorker` | Moves staged envelopes into the durable `gateway_envelopes_*` tables, assigning sequence ids |
| Subscribe worker | `pkg/api/message/subscribe_worker.go`, `startSubscribeWorker` | Polls the DB per originator and fans envelopes out to in-process listeners |
| Cursor updater | `pkg/api/metadata/cursor_updater.go`, `NewCursorUpdater` | Polls `gateway_envelopes_latest` every 100 ms into an in-memory vector clock |
| Indexer **[DROP]** | `pkg/indexer/indexer.go` | Watches the app chain for group-message commits and identity updates, writes them as envelopes |
| Sync **[DROP]** | `pkg/sync/*.go` | Replicates envelopes from peer originator nodes |
| Payer reports **[DROP]** | `pkg/payerreport/*` | Generates, attests, and settles billing reports on chain |
| Gateway / payer **[DROP]** | `pkg/api/payer/service.go`, `pkg/gateway/*` | The client-facing publish entry point that signs and routes to a node or the chain |
| Pruner | `pkg/prune/prune.go`, `Executor.Run` | Deletes expired rows and drops empty partitions |

**Key structural fact for the new backend**: the client-facing publish path in production is
`PayerApi.PublishClientEnvelopes` (or the identical `GatewayApi.PublishClientEnvelopes`), which is a
*separate process* from the node. It signs a `PayerEnvelope`, picks a target node, and forwards to that
node's `ReplicationApi.PublishPayerEnvelopes`. In a single-backend world those two hops collapse into
one.

Server construction and handler registration: `pkg/server/server.go`, `BaseServer.startAPIServer`,
function literal `registrationFunc`.

---

## 2. The envelope model

### 2.1 Layer diagram

```text
OriginatorEnvelope                     (what readers receive)
├── unsigned_originator_envelope: bytes  ──► UnsignedOriginatorEnvelope
│                                             ├── originator_node_id      uint32
│                                             ├── originator_sequence_id  uint64
│                                             ├── originator_ns           int64
│                                             ├── base_fee_picodollars    uint64   [DROP]
│                                             ├── congestion_fee_picodollars uint64 [DROP]
│                                             ├── expiry_unixtime         uint64
│                                             └── payer_envelope_bytes: bytes ──► PayerEnvelope
│                                                     ├── unsigned_client_envelope: bytes ──► ClientEnvelope
│                                                     │        ├── aad: AuthenticatedData
│                                                     │        │      ├── target_topic  bytes
│                                                     │        │      └── depends_on    Cursor
│                                                     │        └── payload: oneof
│                                                     ├── payer_signature          [DROP]
│                                                     ├── target_originator uint32 [DROP]
│                                                     └── message_retention_days uint32
└── proof: oneof
    ├── originator_signature   (node-signed)     [DROP]
    └── blockchain_proof {transaction_hash}      [DROP]
```

Every nesting level is **serialized bytes inside the parent**, not a nested proto message. This means a
reader must `proto.Unmarshal` four times to reach the payload. Source:
`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, and the Go wrappers in `pkg/envelopes/*.go`.

### 2.2 ClientEnvelope

Proto: `pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `ClientEnvelope`.

| Field | # | Type |
| --- | --- | --- |
| `aad` | 1 | `*AuthenticatedData` |
| `payload` | oneof, 2–7 | see below |

Payload oneof variants:

| Variant | Field # | Wrapped type | Required topic kind |
| --- | --- | --- | --- |
| `ClientEnvelope_GroupMessage` | 2 | `*mlsv1.GroupMessageInput` | `TopicKindGroupMessagesV1` (0) |
| `ClientEnvelope_WelcomeMessage` | 3 | `*mlsv1.WelcomeMessageInput` | `TopicKindWelcomeMessagesV1` (1) |
| `ClientEnvelope_UploadKeyPackage` | 4 | `*mlsv1.UploadKeyPackageRequest` | `TopicKindKeyPackagesV1` (3) |
| `ClientEnvelope_IdentityUpdate` | 5 | `*associations.IdentityUpdate` | `TopicKindIdentityUpdatesV1` (2) |
| `ClientEnvelope_PayerReport` **[DROP]** | 6 | `*PayerReport` | `TopicKindPayerReportsV1` (4) |
| `ClientEnvelope_PayerReportAttestation` **[DROP]** | 7 | `*PayerReportAttestation` | `TopicKindPayerReportAttestationsV1` (5) |

Go wrapper: `pkg/envelopes/client.go`, type `ClientEnvelope`.

`NewClientEnvelope(proto)` validates, in order:

1. `proto == nil` → `"client envelope proto is nil"`.
2. `proto.GetAad() == nil` → `"aad is missing"`.
3. `proto.Payload == nil` → `"payload is missing"`.
4. `topic.ParseTopic(proto.GetAad().GetTargetTopic())` must succeed. The parsed topic is stored as
   `c.targetTopic`.

**The target topic is read from the AAD; it is never derived from the payload.** The only link between
the two is the consistency check `ClientEnvelope.TopicMatchesPayload()`
(`pkg/envelopes/client.go`), a type switch that returns `true` only when the AAD topic kind matches
the payload variant per the table above, and `false` for any unrecognized variant.

### 2.3 AuthenticatedData (AAD)

`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `AuthenticatedData`:

| Field | # | Type | Notes |
| --- | --- | --- | --- |
| *(retired)* | 1 | — | was `target_originator`; source comment says do not reuse |
| `target_topic` | 2 | `bytes` | the kind-prefixed topic (see §3) |
| `depends_on` | 3 | `*Cursor` | causal dependency vector |
| *(retired)* | 4 | — | was `is_commit`; do not reuse |

The AAD carries no signature of its own. Its authenticity is transitive: it sits inside the serialized
`ClientEnvelope` bytes that the payer signs.

`depends_on` is validated by `pkg/api/message/service.go`, `Service.validateClientInfo`. For each
`(nodeID, seqID)` pair:

- `nodeID >= 100` → `InvalidArgument`, `"node ID %d specified in DependsOn is not a valid node ID, a
  message can not depend on a non-commit"`. Node ids below 100 are reserved for blockchain-sourced
  originators (`constants.GroupMessageOriginatorID = 0`, `constants.IdentityUpdateOriginatorID = 1`).
- `nodeID` absent from `s.cu.GetCursor()` → `InvalidArgument`, `"node ID %d specified in DependsOn has
  not been seen by this node"`.
- `seqID > lastSeqID` → `InvalidArgument`, `"sequence ID %d for node ID %d specified in DependsOn
  exceeds last seen sequence ID %d"`.

Two `TODO`s remain in that function: the blockchain sequence-id equality check and payload-specific
validation (identity updates) are not implemented.

### 2.4 PayerEnvelope

`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `PayerEnvelope`:

| Field | # | Type | Fate |
| --- | --- | --- | --- |
| `unsigned_client_envelope` | 1 | `bytes` (serialized `ClientEnvelope`) | **[KEEP]** |
| `payer_signature` | 2 | `*associations.RecoverableEcdsaSignature` | **[DROP]** |
| `target_originator` | 3 | `uint32` | **[DROP]** |
| `message_retention_days` | 4 | `uint32` | **[KEEP]** |

Go wrapper: `pkg/envelopes/payer.go`.

`NewPayerEnvelope(proto)`: nil check, then `NewClientEnvelopeFromBytes(proto.GetUnsignedClientEnvelope())`
— so every `ClientEnvelope` validation above also gates `PayerEnvelope` construction. It does **not**
verify the signature.

`PayerEnvelope.RecoverSigner()` recovers an Ethereum address:

```go
hash := utils.HashPayerSignatureInput(p.proto.GetTargetOriginator(), p.proto.GetUnsignedClientEnvelope())
signer, err := ethcrypto.SigToPub(hash, payerSignature.GetBytes())
address := ethcrypto.PubkeyToAddress(*signer)
```

It errors only when `PayerSignature` is nil or the signature bytes are malformed. **A wrong signature
does not fail — it recovers a different address**, which then fails the balance check instead
(`pkg/envelopes/envelopes_test.go`, `TestRecoverSigner`).

### 2.5 UnsignedOriginatorEnvelope

`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `UnsignedOriginatorEnvelope`:

| Field | # | Type | Fate |
| --- | --- | --- | --- |
| `originator_node_id` | 1 | `uint32` | **[DROP]** |
| `originator_sequence_id` | 2 | `uint64` | **[KEEP]** — becomes the single sequence |
| `originator_ns` | 3 | `int64` | **[KEEP]** — publish time, nanoseconds |
| `payer_envelope_bytes` | 4 | `bytes` | **[KEEP]** (or flatten) |
| `base_fee_picodollars` | 5 | `uint64` | **[DROP]** |
| `congestion_fee_picodollars` | 6 | `uint64` | **[DROP]** |
| `expiry_unixtime` | 7 | `uint64` | **[KEEP]** |

Go wrapper: `pkg/envelopes/unsigned_originator.go`. Construction parses `payer_envelope_bytes`; the
doc comment says explicitly "Does not verify signatures."

### 2.6 OriginatorEnvelope

`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `OriginatorEnvelope`:

| Field | # | Type |
| --- | --- | --- |
| `unsigned_originator_envelope` | 1 | `bytes` |
| `proof` | oneof 2–3 | `originator_signature` (`*RecoverableEcdsaSignature`) **or** `blockchain_proof` (`*BlockchainProof{transaction_hash bytes}`) |

Go wrapper: `pkg/envelopes/originator.go`. Also does **not** verify the signature.

**Notable gap**: there is no originator-signature *verification* function anywhere under `pkg/`. Only
construction (`pkg/registrant/registrant.go`, `SignStagedEnvelope`) and the hash helper
(`pkg/utils/hash.go`, `HashOriginatorSignatureInput`) exist. A reading client is expected to look the
originator's key up in the node registry and verify, but xmtpd itself never does.

### 2.7 Cursor

`pkg/proto/xmtpv4/envelopes/envelopes.pb.go`, type `Cursor`:

| Field | # | Type |
| --- | --- | --- |
| `node_id_to_sequence_id` | 1 | `map<uint32, uint64>` |

This is the **vector clock** that appears in every query, subscribe, and metadata call. Semantics:

- An envelope is delivered iff `envelope.originator_sequence_id > cursor[envelope.originator_node_id]`.
- An originator **absent** from the map is treated as sequence `0` — but only after the server fills it
  in. See §7.3 for the important subtlety.
- "From the beginning" is an **empty map**, not `0`.
- Go alias: `db.VectorClock = map[uint32]uint64` (`pkg/db/types.go`). Per-topic form:
  `db.TopicCursors = map[string]VectorClock`.

**[DROP]** With no originators, this collapses to a scalar `u64`.

### 2.8 Signature scheme summary

All signatures are secp256k1 recoverable ECDSA (65 bytes, r‖s‖v), from `go-ethereum`'s `ethcrypto`.
Domain-separation labels are in `pkg/constants/constants.go`.

| Signer | Field | Hashed bytes | Helper |
| --- | --- | --- | --- |
| Payer | `PayerEnvelope.payer_signature` | `keccak256("target\|" ‖ be_u32(target_originator) ‖ "payer\|" ‖ unsigned_client_envelope)` | `pkg/utils/hash.go`, `HashPayerSignatureInput`; signed by `pkg/utils/signature.go`, `SignClientEnvelope` |
| Node | `OriginatorEnvelope.originator_signature` | `keccak256("originator\|" ‖ unsigned_originator_envelope_bytes)` | `pkg/utils/hash.go`, `HashOriginatorSignatureInput`; signed by `pkg/registrant/registrant.go`, `Registrant.SignStagedEnvelope` |
| Node (auth JWT) | `node-authorization` header | `keccak256("jwt\|" ‖ signing_input)` | `pkg/utils/hash.go`, `HashJWTSignatureInput` |

### 2.9 What a single-Postgres backend does not need

| Machinery | Why it exists in xmtpd | Verdict |
| --- | --- | --- |
| `payer_signature`, `RecoverSigner`, `HashPayerSignatureInput` | proves who pays for a message in a paid, multi-operator network | **DROP** — no payer service |
| `target_originator` | a payer addresses one specific node | **DROP** — one backend |
| `originator_node_id` everywhere | multiple independent writers, no global sequence | **DROP** — one writer, one sequence |
| `originator_signature` / `blockchain_proof` | proves a node or the chain produced the envelope | **DROP** — trusted single backend |
| `base_fee_picodollars`, `congestion_fee_picodollars` | metering for on-chain settlement | **DROP** |
| The whole `Cursor` map type | per-originator vector clocks | **DROP** → scalar `u64` |
| `depends_on` causal vector | orders commits against a partially-replicated view | **DROP** or radically simplify — one DB gives real ordering |
| Four levels of byte-nesting | each layer is independently signed and forwarded | **SIMPLIFY** — collapse to at most two |
| `expiry_unixtime`, `message_retention_days` | storage retention | **KEEP** |
| `target_topic` in AAD + `TopicMatchesPayload` | routing and payload/topic consistency | **KEEP** |
| `originator_sequence_id`, `originator_ns` | ordering and timestamps | **KEEP**, renamed |

---

## 3. Topics

Source: `pkg/topic/topic.go`, `pkg/topic/topic_test.go`.

### 3.1 Byte layout

```text
[ 1 byte: TopicKind ][ N bytes: identifier (opaque) ]
```

`Topic.Bytes()` allocates `1 + len(identifier)`, sets byte 0 to `byte(kind)`, and copies the identifier
into bytes `1:`.

### 3.2 Kinds

`type TopicKind uint8`, defined with `iota`:

| Constant | Value | `String()` | Identifier | Fate |
| --- | --- | --- | --- | --- |
| `TopicKindGroupMessagesV1` | 0 | `group_messages_v1` | group id (16 bytes) | KEEP |
| `TopicKindWelcomeMessagesV1` | 1 | `welcome_message_v1` | installation key | KEEP |
| `TopicKindIdentityUpdatesV1` | 2 | `identity_updates_v1` | inbox id (32 bytes) | KEEP |
| `TopicKindKeyPackagesV1` | 3 | `key_packages_v1` | installation key | KEEP |
| `TopicKindPayerReportsV1` | 4 | `payer_reports_v1` | report id | **DROP** |
| `TopicKindPayerReportAttestationsV1` | 5 | `payer_report_attestations_v1` | report id | **DROP** |

`TopicKind.String()` returns `"unknown"` for anything else, which is how `ParseTopic` detects bad kinds.

### 3.3 Parsing

`topic.ParseTopic(b []byte) (*Topic, error)`:

| Condition | Error |
| --- | --- |
| `len(b) < 2` | `"topic must be at least 2 bytes long"` |
| `TopicKind(b[0]).String() == "unknown"` | `"unknown topic kind %d"` |

Note the minimum of **2** bytes: a zero-length identifier is rejected. There is **no maximum** enforced
in `ParseTopic`; the API layer imposes `maxTopicLength = 128` separately
(`pkg/api/message/service.go`).

### 3.4 String form

`Topic.String()` returns `fmt.Sprintf("%s/%x", t.kind.String(), t.identifier)`, e.g.
`group_messages_v1/deadbeef`. This string is the key used by the subscribe worker's topic dispatch map
(`pkg/api/message/subscribe_worker.go`, `dispatchToTopics`), while the **raw bytes** are the key used
by the SQL queries and the per-topic cursor maps. Both keyings coexist in `subscribeTopic`
(`pkg/api/message/subscribe.go`): `cursorKey` (raw bytes as a string) and `listenKey` (the parsed
string form).

### 3.5 Reserved topics

`Topic.IsReserved()` returns true for `TopicKindPayerReportsV1` and
`TopicKindPayerReportAttestationsV1`. Comment: "IsReserved topics can only be published to by the node
itself, and not through Payers." Enforced in `pkg/api/message/service.go`,
`Service.preprocessPayerEnvelopes`. **[DROP]** — no reserved topics remain without payer reports.

---

## 4. Retention and expiry

### 4.1 The chain

1. **Payer picks retention.** `pkg/api/payer/service.go`, `determineRetentionPolicy(clientEnvelope)`
   returns `constants.DefaultStorageDurationDays` = **60** for every topic kind. It `panic`s for
   identity updates and for group-message commits/proposals (both unreachable, since those go to the
   chain). A `TODO(mkysel)` notes that welcome- and key-package-specific policies are not implemented.
   The value goes into `PayerEnvelope.message_retention_days`.

2. **Node bounds-checks it.** `pkg/api/message/service.go`, `Service.validateExpiry`:

   | Condition | Code | Message |
   | --- | --- | --- |
   | `RetentionDays() < 2` | `InvalidArgument` | `invalid expiry retention days. Must be >= 2` |
   | `RetentionDays() != math.MaxUint32 && RetentionDays() > 365` | `InvalidArgument` | `invalid expiry retention days. Must be <= 365` |

   Note the escape hatch: **exactly `math.MaxUint32`** bypasses the upper bound. The migrator's
   `math.MaxInt32` default does *not* qualify.

3. **Node computes the absolute expiry at signing time.** `pkg/registrant/registrant.go`,
   `Registrant.SignStagedEnvelope`:

   ```go
   ExpiryUnixtime: uint64(time.Now().UTC().Add(time.Hour * 24 * time.Duration(retentionDays)).Unix())
   ```

   So **expiry = signing wall-clock + retentionDays × 24 h**, in Unix seconds.

4. **Stored as a column.** `pkg/api/message/publish_worker.go`, `prepareSingleEnvelope` reads
   `validatedEnvelope.UnsignedOriginatorEnvelope.Proto().GetExpiryUnixtime()` into
   `preparedEnvelope.expiry`, and `persistBatch` writes it as `gateway_envelopes_meta.expiry`
   (`bigint`).

5. **Blockchain-sourced envelopes never expire.** `pkg/indexer/app_chain/contracts/group_message_storer.go`
   and `identity_update_storer.go` insert with `Expiry: math.MaxInt64`.

### 4.2 Enforcement

Expiry is **not** enforced on read. An expired row is still returned by every query and subscribe path
until the pruner deletes it (§15). This is worth noting: the effective retention is
"expiry, plus however long until the pruner next runs and the payer report ceiling has advanced past
it."

---

## 5. gRPC service surface

Every procedure actually registered, from `pkg/proto/xmtpv4/*/**connect/*.go` and confirmed against
`pkg/server/server.go`, `registrationFunc`:

| Service | Procedure | Kind | Handler |
| --- | --- | --- | --- |
| `ReplicationApi` | `PublishPayerEnvelopes` | unary | `pkg/api/message/service.go`, `Service.PublishPayerEnvelopes` |
| `ReplicationApi` | `QueryEnvelopes` | unary | `Service.QueryEnvelopes` |
| `ReplicationApi` | `SubscribeEnvelopes` | server stream | `Service.SubscribeEnvelopes` |
| `ReplicationApi` | `SubscribeTopics` | server stream | `Service.SubscribeTopics` |
| `ReplicationApi` | `SubscribeOriginators` | server stream | `Service.SubscribeOriginators` |
| `ReplicationApi` | `GetInboxIds` | unary | `Service.GetInboxIds` |
| `ReplicationApi` | `GetNewestEnvelope` | unary | `Service.GetNewestEnvelope` |
| `QueryApi` | `QueryEnvelopes` | unary | same `Service.QueryEnvelopes` |
| `QueryApi` | `SubscribeTopics` | server stream | same `Service.SubscribeTopics` |
| `QueryApi` | **`Subscribe`** | **bidi stream** | `pkg/api/message/subscribe.go`, `Service.Subscribe` |
| `QueryApi` | `GetInboxIds` | unary | same |
| `QueryApi` | `GetNewestEnvelope` | unary | same |
| `PublishApi` | `PublishPayerEnvelopes` | unary | same `Service.PublishPayerEnvelopes` |
| `NotificationApi` | `SubscribeAllEnvelopes` | server stream | `Service.SubscribeAllEnvelopes` |
| `MetadataApi` | `GetSyncCursor` | unary | `pkg/api/metadata/service.go`, `Service.GetSyncCursor` |
| `MetadataApi` | `SubscribeSyncCursor` | server stream | `Service.SubscribeSyncCursor` |
| `MetadataApi` | `GetVersion` | unary | `Service.GetVersion` |
| `MetadataApi` | `GetPayerInfo` | unary | `Service.GetPayerInfo` |
| `PayerApi` **[DROP]** | `PublishClientEnvelopes` | unary | `pkg/api/payer/service.go`, `Service.PublishClientEnvelopes` |
| `PayerApi` **[DROP]** | `GetNodes` | unary | `Service.GetNodes` |
| `GatewayApi` **[DROP]** | `PublishClientEnvelopes` | unary | same |
| `GatewayApi` **[DROP]** | `GetNodes` | unary | same |
| `MisbehaviorApi` | `SubmitMisbehaviorReport`, `QueryMisbehaviorReports` | — | **generated proto only; never registered** |

**`MisbehaviorApi` is not the only generated-but-unregistered service.** The tree contains connect
handlers and clients for several legacy/adjacent APIs that `registrationFunc` never mounts. Their
procedures, request/response types, and client stubs all exist and compile; only the server
registration is absent. A replacement backend does not have to serve any of them, but should know
they exist, because clients in the wider ecosystem are generated against them:

| Generated service | Package | Procedures | Registered? |
| --- | --- | --- | --- |
| `MisbehaviorApi` | `pkg/proto/xmtpv4/message_api/misbehavior_api*.pb.go` | `SubmitMisbehaviorReport`, `QueryMisbehaviorReports` | no |
| `MlsApi` (v3) | `pkg/proto/mls/api/v1/apiv1connect/mls.connect.go` | `SendGroupMessages`, `SendWelcomeMessages`, `RegisterInstallation`, `UploadKeyPackage`, `FetchKeyPackages`, `RevokeInstallation`, `GetIdentityUpdates`, `QueryGroupMessages`, `QueryWelcomeMessages`, `SubscribeGroupMessages`, `SubscribeWelcomeMessages`, `Subscribe`, **`BatchPublishCommitLog`**, **`BatchQueryCommitLog`**, `GetNewestGroupMessage` | no |
| `IdentityApi` (v3) | `pkg/proto/identity/api/v1/apiv1connect/identity.connect.go` | `PublishIdentityUpdate`, `GetIdentityUpdates`, `GetInboxIds`, `VerifySmartContractWalletSignatures` | no |
| `MessageApi` (v1, pre-MLS) | `pkg/proto/message_api/v1/message_apiv1connect/message_api.connect.go` | `Publish`, `Subscribe`, `Subscribe2`, `SubscribeAll`, `Query`, `BatchQuery` | no |
| `D14nMigrationApi` | `pkg/proto/migration/api/v1/apiv1connect/migration.connect.go` | `FetchD14nCutover` | no |

**Correction to a claim made elsewhere in this document**: §11.2 previously said "there is no commit
log RPC anywhere in this tree." That is **wrong**. The generated v3 `MlsApi` defines **two**:
`/xmtp.mls.api.v1.MlsApi/BatchPublishCommitLog` and `/xmtp.mls.api.v1.MlsApi/BatchQueryCommitLog`,
with request types `BatchPublishCommitLogRequest` and `BatchQueryCommitLogRequest` (the publish RPC
returns `emptypb.Empty`). What is true is narrower: **xmtpd registers no handler for either**, and
there is no commit-log table, no storage, and no xmtpd-side implementation. The concept exists in the
protos this repository vendors; the *service* does not run here.

One `Service` struct (`pkg/api/message/service.go`) implements `ReplicationApiHandler`,
`QueryApiHandler`, `PublishApiHandler`, and `NotificationApiHandler` simultaneously — the four service
names are four *facades* over the same handler set, differing only in which interceptors wrap them.

**There is no `GetReaderNode`, no `BatchSubscribe`, no `GetPayerInfo` on the payer API.** Those names in
the research brief do not exist in this tree.

Health (`grpchealth`) and reflection (`grpcreflect`, both v1 and v1alpha, gated by
`--reflection.enable`) are also registered: `pkg/api/server.go`, `registerHealthHandler` and
`registerReflectionHandlers`.

CORS: `pkg/api/server.go`, `handleCORS` — `Access-Control-Allow-Origin: *`, exposes
`grpc-status,grpc-message,grpc-status-details-bin`, 24 h preflight cache.

HTTP server timeouts (`pkg/api/server.go`, `NewAPIServer`): `IdleTimeout` 5 min (both `http.Server` and
`http2.Server`), `ReadHeaderTimeout` 10 s, `ReadTimeout` 30 s. No write timeout, which is what makes
long-lived streams possible.

Message size caps — **node APIs only**. `pkg/server/server.go`, `registrationFunc`, builds
`handlerOpts` with `connect.WithReadMaxBytes(constants.GRPCPayloadLimit)` and
`connect.WithSendMaxBytes(constants.GRPCPayloadLimit)`, where
`GRPCPayloadLimit = 25 * 1024 * 1024` (25 MiB) (`pkg/constants/constants.go`). The comment says it
must stay in sync with `libxmtp/crates/xmtp_configuration/src/common/api.rs`. Those options are
applied to `ReplicationApi`, `QueryApi`, `PublishApi`, `NotificationApi`, and `MetadataApi`
(`queryHandlerOpts` re-lists both options when it adds the rate-limit interceptor).

**The gateway has no such cap.** `pkg/gateway/builder.go` registers `PayerApi` and `GatewayApi` with
`connect.WithInterceptors(interceptors...)` **only** — no `WithReadMaxBytes`, no `WithSendMaxBytes`.
Connect treats a zero/unset value as **unlimited** (`connectrpc.com/connect`, `option.go`), so
gateway ingress and egress have no repository-level per-message limit. The gateway is a separate
process from the node, so the two are configured independently. The only size check on the gateway
path is `s.maxPayerMessageSize` (default 200 000 bytes), and it applies **only to blockchain-bound
envelopes** (§6.11).

So an oversized client publish is refused at the *node* hop, not at the gateway hop — the gateway
accepts it, then fails forwarding it. **[FIX]** — cap at ingress, where the client can be told why.

---

## 6. Endpoint reference

Shared constants used throughout, all from `pkg/api/message/service.go` unless noted:

```go
maxRequestedRows      int32 = 1000    // default and max page size for QueryEnvelopes
maxInboxIdsPerRequest int   = 1000    // GetInboxIds
maxQueriesPerRequest  int   = 10000   // topics + originators in one query/subscribe filter
maxTopicLength        int   = 128     // bytes
maxVectorClockLength  int   = 100     // cursor map entries
pagingInterval              = 100 * time.Millisecond  // sleep between catch-up pages
requestMissingMessageError  = "missing request message"
```

From `pkg/api/message/subscribe_topics.go`:

```go
maxTopicsPerChunk int   = 500    // SubscribeTopics catch-up chunking
topicPageLimit    int32 = 500    // rows per catch-up page
maxTopicFilters   int   = 10_000 // SubscribeTopics filters
```

Most unary handlers begin with the same nil guard:

```go
if req.Msg == nil {
    return connect.NewError(connect.CodeInvalidArgument, errors.New(requestMissingMessageError))
}
```

I do not repeat that row in each error table below; assume it for every endpoint **that has the
guard**. Four handlers do **not** check `req.Msg`: `PayerApi/GatewayApi.GetNodes`
(`pkg/api/payer/service.go`, `Service.GetNodes`), `MetadataApi.GetSyncCursor`,
`MetadataApi.SubscribeSyncCursor`, and `MetadataApi.GetVersion` (`pkg/api/metadata/service.go`).
`SubscribeSyncCursor` ignores its request object entirely (parameter named `_`). The bidi
`QueryApi.Subscribe` (`pkg/api/message/subscribe.go`, `Service.Subscribe`) has no unary request
object at all, so the guard cannot apply. Only handlers with an explicit guard return
`missing request message`.

---

### 6.0 IMPORTANT — the wire message is not the handler message

**Read this before any error table in this document.** Every error table in §6 lists the message the
*handler* constructs. Most of those messages **never reach the client**. The logging interceptor
rewrites them.

`pkg/interceptors/server/logging.go`, `sanitizeError`, is installed on **every** RPC by
`pkg/api/server.go`, `NewAPIServer` (it is appended to `serverInterceptors` and passed to
`cfg.RegistrationFunc`, so it wraps every registered service). It logs the real error, then returns a
sanitized one:

| Handler error | Wire code | Wire message |
| --- | --- | --- |
| `context.DeadlineExceeded` | `DeadlineExceeded` | `request timed out` |
| `context.Canceled` | `Canceled` | `request was canceled` |
| `InvalidArgument` | `InvalidArgument` | **handler text, verbatim** |
| `Unimplemented` | `Unimplemented` | **handler text, verbatim** |
| `NotFound` | `NotFound` | **handler text, verbatim** |
| `Internal` | `Internal` | `internal server error` |
| any other connect code (`ResourceExhausted`, `Unavailable`, `Aborted`, `FailedPrecondition`, `PermissionDenied`, `Unauthenticated`, …) | **code preserved** | `request has failed` |
| a bare Go error (no `connect.NewError`) | `Unknown` | `unknown error` |

**The rule in one line: only `InvalidArgument`, `Unimplemented`, and `NotFound` keep the handler's
text. Every other code keeps its *code* but loses its *message*.**

To make this readable, every error table below carries a **Wire message** column:

- **verbatim** — the client sees the handler message shown in that row.
- `internal server error` — the handler message is discarded (code `Internal`).
- `request has failed` — the handler message is discarded (any other connect code).
- `unknown error` — bare error, code becomes `Unknown`.

**Implication for the new backend**: the detailed messages in these tables are for *log* reading, not
for client contract. A client can only branch on the gRPC code, plus the text of the three preserved
codes. If the new backend wants clients to distinguish, say, "consumer too slow" from "shutting
down", it must either use a preserved code or carry a structured detail — the text alone will not
survive.

---

### 6.1 ReplicationApi.PublishPayerEnvelopes

`pkg/api/message/service.go`, `Service.PublishPayerEnvelopes`. This is the **node ingress** — the only
way an envelope enters the durable store other than the blockchain indexer.

#### Request

```protobuf
message PublishPayerEnvelopesRequest {
  repeated PayerEnvelope payer_envelopes = 1;
}
```

#### Validation

The full pipeline, in execution order:

**Step 0 — request-level gates** (`PublishPayerEnvelopes` itself):

1. `s.migrationEnabled` → reject. **[DROP]**
2. `len(payerEnvelopes) == 0` → reject.

**Step 1 — per-envelope** (`Service.preprocessPayerEnvelopes`, loops over all envelopes, **accumulating**
errors rather than failing fast, then joins them with `\n`):

1. `Service.validatePayerEnvelope`:
   - `envelopes.NewPayerEnvelope(rawEnv)` — nil check + `ClientEnvelope` parse (nil proto, missing AAD,
     missing payload, unparseable topic).
   - `payerEnv.TargetOriginator != s.registrant.NodeID()` → `"invalid target originator"`. **[DROP]**
   - `payerEnv.RecoverSigner()` → yields the payer address. **[DROP]**
   - `Service.validateClientInfo`: AAD non-nil; `TopicMatchesPayload()`; `depends_on` checks (§2.3).
   - `Service.validateExpiry`: retention days in `[2, 365]` or exactly `MaxUint32` (§4.1).
2. Re-marshal via `payerEnvelope.Bytes()`.
3. `targetTopic.IsReserved()` → reject. **[DROP]**
4. `topicKind == TopicKindIdentityUpdatesV1` → reject: identity updates must go through the chain.
   **[DROP]**
5. `topicKind == TopicKindGroupMessagesV1` → `Service.validateGroupMessage`:
   - payload must be `ClientEnvelope_GroupMessage`;
   - `deserializer.ShouldSendToBlockchain(payload)` (`pkg/deserializer/deserializer.go`) deserializes
     the MLS message and inspects its content type; `true` for **Commit** or **Proposal**;
   - if true → reject: commits and proposals must go through the chain. **[DROP]**
6. `topicKind == TopicKindKeyPackagesV1` → `Service.validateKeyPackage`: calls the MLS validation
   service `ValidateKeyPackages` with the TLS-serialized key package (§11).
7. Fee computation. **[DROP]** `s.feeCalculator.CalculateBaseFee(now, len(envelopeBytes),
   retentionDays)` and `batchCalc.CalculateCongestionFee(now)`. Unlike the other steps these return
   immediately on error rather than accumulating — a fee failure aborts the whole request, it does not
   skip one envelope.

**How `preprocessPayerEnvelopes` errors reach the client — an important flattening.**
`preprocessPayerEnvelopes` (`pkg/api/message/service.go`) returns a **plain Go error**, never a
`connect.Error`. Every per-envelope failure is turned into a *string* and appended to `errs`; the
accumulated strings are joined and returned. The two fee paths return a plain
`fmt.Errorf("could not calculate base fee for envelope %d: %w", i, err)` /
`fmt.Errorf("could not calculate congestion fee for envelope %d: %w", i, err)`.

The **caller** then wraps whatever came back in one code:

```go
processedEnvelopes, err := s.preprocessPayerEnvelopes(ctx, payerEnvelopes)
if err != nil {
    return nil, connect.NewError(
        connect.CodeInvalidArgument,
        fmt.Errorf("error processing payer envelopes:%w", err),
    )
}
```

**Consequence**: any code a step *thought* it was returning is discarded. `validateKeyPackage`
constructs `connect.CodeInternal` for an unreachable MLS service and for an empty validation result,
but those never survive — the error is stringified into `could not validate key package. index %d:
%v` and the handler returns `InvalidArgument`. The same is true of the fee failures, which can be
genuine internal faults (a failed DB read inside the congestion calculator) yet reach the client as
`InvalidArgument`. Because `InvalidArgument` is one of the three preserved codes (§6.0), the whole
composite string **does** reach the client verbatim.

This matters for the new backend: an internal fault is being reported to the client as its own bad
input. Keep the accumulate-and-report-all-indexes behavior — it is genuinely useful — but classify
each accumulated failure so a transport or database fault does not masquerade as `InvalidArgument`.

**Step 2 — batch-level**: all envelopes must share one recovered payer address, else
`"all envelopes in a request must be from the same payer"`. **[DROP]**

**Step 3 — balance** (only when `APIOptions.RequirePayerPositiveBalance`): `Service.checkPayerBalance`
sums `BaseFee + CongestionFee` and compares to `ledger balance − unsettled usage`
(`Service.getAvailableBalance`, which calls `ledger.FindOrCreatePayer`, `ledger.GetBalance`, and
`GetPayerUnsettledUsage`). **[DROP]**

**Notably absent**: welcome messages get no validation beyond the generic AAD/topic checks. Non-commit
group messages are never sent to the MLS validation service —
`MLSValidationService.ValidateGroupMessages` exists but has **no caller** outside its own tests.

#### Storage

Two-phase, and the phase split is the heart of xmtpd's ordering guarantee.

**Phase A — staging, synchronous** (`Service.criticalPathDBInsert`): one call to
`InsertStagedOriginatorEnvelopeBatch`, which invokes the plpgsql function
`insert_staged_originator_envelope_batch_v2(topics, payer_envelopes)`
(`pkg/db/migrations/00021_insert_staged_envelopes_batch-v2.up.sql`). That function:

1. Takes `pg_advisory_xact_lock(hashtext('staged_originator_envelopes_sequence'))` — a
   **content-derived** advisory lock that serializes all concurrent stagers.
2. Inserts all rows `ORDER BY input.i`, preserving caller array order, with `ON CONFLICT DO NOTHING`.
3. Returns each row's assigned `(id, originator_time, topic, payer_envelope)`.

The lock exists because a bare `BIGSERIAL` does **not** guarantee that commit order matches id order
under concurrency. Serializing the insert makes `staged_originator_envelopes.id` a true monotonic
insertion counter. This is the single most important mechanism to carry forward.

The handler then verifies `len(insertedStaged) == len(processedEnvelopes)`, else an internal error.

**Phase B — publish, asynchronous** (`pkg/api/message/publish_worker.go`): see §7.2. The handler does
not wait for it to complete before signing the response, but it *does* wait before returning.

#### Response

```protobuf
message PublishPayerEnvelopesResponse {
  repeated OriginatorEnvelope originator_envelopes = 1;
}
```

Each is produced by `Registrant.SignStagedEnvelope(stagedEnvelope, baseFee, congestionFee,
retentionDays)` — assigning `originator_node_id`, `originator_sequence_id = staged.ID`,
`originator_ns = staged.OriginatorTime.UnixNano()`, the fees, `expiry_unixtime`, and the node signature.

**The returned envelope is not the stored envelope.** This is a real and easily-missed property. The
response and the durable row are signed **twice, independently, by two different code paths**:

| | Response envelope | Stored envelope |
| --- | --- | --- |
| Signed by | `Service.PublishPayerEnvelopes` (the handler), `pkg/api/message/service.go` | `publishWorker.prepareSingleEnvelope`, `pkg/api/message/publish_worker.go` |
| Fees | computed in `preprocessPayerEnvelopes` at `now := time.Now()`, before staging | **recomputed** in `prepareSingleEnvelope` at `stagedEnv.OriginatorTime`, and forced to `0` for reserved topics |
| Expiry | `time.Now()` inside `SignStagedEnvelope` (`pkg/registrant/registrant.go`) + retention | `time.Now()` inside the worker's own `SignStagedEnvelope` call + retention — a **later** wall-clock reading |
| Signature | over the handler's unsigned bytes | over the worker's unsigned bytes |

Because `SignStagedEnvelope` reads `time.Now()` each time it is called, and the worker calls it after
the handler, the two envelopes carry **different `expiry_unixtime`** whenever a second boundary is
crossed, and therefore **different unsigned bytes and different signatures**. The congestion fee can
also differ, because it is recomputed against a different minute bucket.

What is guaranteed to match: `topic`, the payer envelope bytes, `originator_node_id`,
`originator_sequence_id` (both use `staged.ID`), and `originator_ns` (both use
`staged.OriginatorTime`).

What can differ: `base_fee_picodollars`, `congestion_fee_picodollars`, `expiry_unixtime`, the
serialized `unsigned_originator_envelope` bytes, and the `originator_signature`.

**Consequence**: a client that stores the publish response and later compares it byte-for-byte with
the same envelope read back from `QueryEnvelopes` will find a mismatch, and any signature it verified
on the response is not the signature the store holds. **[FIX]** — the new backend should sign once,
inside the transaction that assigns the sequence, and return exactly the bytes it stored.

Before returning, `Service.waitForGatewayPublish` polls `s.publishWorker.lastProcessed` every **10 ms**
until it reaches the last staged id, with a **30 s** timeout. On timeout it logs a warning and returns
successfully anyway — so a slow publish worker yields a response whose envelopes are not yet queryable.

#### Errors

Rows marked "inside composite" are **not** returned as their own error. They are stringified into the
one `error processing payer envelopes:` message, which the handler returns as `InvalidArgument` — so
their text does reach the client, but embedded in that composite string, and the code the step wrote
is discarded (see the note above §6.1's response section).

| Condition | Handler code | Handler message | Wire message |
| --- | --- | --- | --- |
| `req.Msg == nil` | `InvalidArgument` | `missing request message` | **verbatim** |
| Migration enabled | `Internal` | `D14N API is read-only while migration is enabled` | `internal server error` |
| No envelopes | `InvalidArgument` | `missing payer envelope` | **verbatim** |
| Any per-envelope validation failure | `InvalidArgument` | `error processing payer envelopes:` + newline-joined `could not validate envelope. index %d: %v` etc. | **verbatim** |
| Unparseable payer envelope | inside composite | `could not unmarshal payer envelope: %w` | inside the composite, verbatim |
| Wrong target originator | inside composite | `invalid target originator` | inside the composite, verbatim |
| Signature recovery failed | inside composite | `could not recover signer: %w` | inside the composite, verbatim |
| AAD missing | inside composite | `authenticated data is missing` | inside the composite, verbatim |
| Topic/payload mismatch | inside composite | `topic does not match payload` | inside the composite, verbatim |
| `depends_on` node id ≥ 100 | inside composite | `node ID %d specified in DependsOn is not a valid node ID, a message can not depend on a non-commit` | inside the composite, verbatim |
| `depends_on` unknown node | inside composite | `node ID %d specified in DependsOn has not been seen by this node` | inside the composite, verbatim |
| `depends_on` sequence too high | inside composite | `sequence ID %d for node ID %d specified in DependsOn exceeds last seen sequence ID %d` | inside the composite, verbatim |
| Retention < 2 days | inside composite | `invalid expiry retention days. Must be >= 2` | inside the composite, verbatim |
| Retention > 365 days (and ≠ MaxUint32) | inside composite | `invalid expiry retention days. Must be <= 365` | inside the composite, verbatim |
| Reserved topic | inside composite | `reserved topics cannot be published to by gateways. index %d` | inside the composite, verbatim |
| Identity update topic | inside composite | `identity updates must be published via the blockchain. index %d` | inside the composite, verbatim |
| Group message not deserializable | inside composite | `could not validate group message. index %d: invalid group message` | inside the composite, verbatim |
| Commit or proposal | inside composite | `commit and proposal messages must be published via the blockchain` | inside the composite, verbatim |
| Key-package payload type wrong | inside composite | `could not validate key package. index %d: invalid payload type` | inside the composite, verbatim |
| **Key-package MLS service unreachable** | **`InvalidArgument`** (the step writes `Internal`; it is discarded) | `could not validate key package. index %d: could not validate key package: %w` | inside the composite, verbatim |
| **Key-package MLS service empty result** | **`InvalidArgument`** (the step writes `Internal`; it is discarded) | `could not validate key package. index %d: no validation results` | inside the composite, verbatim |
| Key package invalid | inside composite | `key package validation failed: %s` | inside the composite, verbatim |
| **Base-fee calculation failed** | **`InvalidArgument`** (returns immediately, aborts the request) | `error processing payer envelopes:could not calculate base fee for envelope %d: %w` | **verbatim** |
| **Congestion-fee calculation failed** | **`InvalidArgument`** (returns immediately, aborts the request) | `error processing payer envelopes:could not calculate congestion fee for envelope %d: %w` | **verbatim** |
| Mixed payers in one request | `InvalidArgument` | `all envelopes in a request must be from the same payer` | **verbatim** |
| Insufficient balance | `FailedPrecondition` | `insufficient payer balance: available %d picodollars, estimated fees %d picodollars` | `request has failed` |
| Staging insert failed | *(bare error, no code)* | `could not insert staged envelopes: %w` | `unknown error` |
| Staged row count mismatch | *(bare error)* | `expected %d staged envelopes, got %d` | `unknown error` |
| Signing failed | *(bare error)* | `could not sign envelope: %w` | `unknown error` |

Bare errors (no `connect.NewError`) become `CodeUnknown` at the wire, then get rewritten to
`"unknown error"` by the logging interceptor (§6.0, §14.3).

Note the two fee rows: they are **not** accumulated per envelope. A fee failure on envelope 7 of 20
returns immediately and no envelope is staged. They can also represent internal faults (a DB read
inside the congestion calculator) reported to the client as `InvalidArgument`.

#### Limits

| Limit | Value | Enforced where |
| --- | --- | --- |
| Max envelopes per request | **none** | — the only bound is the 25 MiB message size |
| Max payload size | 25 MiB total request | `connect.WithReadMaxBytes(constants.GRPCPayloadLimit)` |
| Publish worker batch | 100 rows | `numRowsPerBatch`, `pkg/api/message/publish_worker.go` |
| Wait-for-publish timeout | 30 s, polled at 10 ms | `Service.waitForGatewayPublish` |

#### Notes

- **Idempotency**: both the staged insert and the gateway insert use `ON CONFLICT DO NOTHING`, but the
  staged table's conflict target is its `BIGSERIAL` primary key, which is freshly generated — so a
  retried publish creates **duplicate** envelopes with new sequence ids. There is no content-hash
  dedupe anywhere. **This is a real gap the new backend should close.**
- The response is signed and returned even if the wait times out.
- Tracing spans: `tracing.SpanNodePublishPayerEnvelopes` wraps the handler,
  `tracing.SpanNodeWaitGatewayPublish` the wait.

---

### 6.2 ReplicationApi/QueryApi.QueryEnvelopes

`pkg/api/message/service.go`, `Service.QueryEnvelopes`.

#### Request

```protobuf
message QueryEnvelopesRequest {
  EnvelopesQuery query = 1;
  uint32         limit = 2;
}
message EnvelopesQuery {
  repeated bytes  topics              = 1;
  repeated uint32 originator_node_ids = 2;
  Cursor          last_seen           = 3;
}
```

#### Validation

`Service.validateQuery(query, allowEmpty=true)`:

| Rule | Error |
| --- | --- |
| both `topics` and `originator_node_ids` non-empty | `cannot filter by both topic and originator in same subscription request` |
| `len(topics) + len(originators) > maxQueriesPerRequest` (10 000) | `too many subscriptions: %d, consider subscribing to fewer topics or subscribing without a filter` |
| any topic empty or > 128 bytes | `invalid topic: %s` |
| `len(cursor) > maxVectorClockLength` (100) | `vector clock length exceeds maximum of %d` |

`allowEmpty=true` for query, so an empty filter is legal — and returns nothing (`fetchEnvelopes`'s
final fallback: "Compatibility with V3, if no filters are set — return nothing").

**Topics are not parsed here.** `ParseTopic` is never called on a query topic; only the length is
checked. An unparseable topic simply matches no rows.

#### Limit resolution

```go
if req.Msg.GetLimit() > uint32(maxRequestedRows) || req.Msg.GetLimit() == 0 {
    limit = maxRequestedRows   // 1000
} else {
    limit = int32(req.Msg.GetLimit())
}
```

So **0 means 1000**, and anything above 1000 is silently clamped to 1000 — not an error.

#### Storage / query selection

`Service.fetchEnvelopes` branches on the filter shape:

| Filter | sqlc query | Notes |
| --- | --- | --- |
| `len(topics) > 0` | `SelectGatewayEnvelopesByTopics` | first calls `s.originatorList.GetOriginatorNodeIDs(ctx)` and `db.FillMissingOriginators(vc, allOriginators)` — see §7.3 |
| exactly 1 originator | `SelectGatewayEnvelopesBySingleOriginator` | simple index scan, `originator_sequence_id > cursor` |
| > 1 originator | `SelectGatewayEnvelopesByOriginators` | LATERAL, one probe per originator |
| neither | *(none)* | returns an empty slice |

All wrapped in `Service.fetchEnvelopesWithRetry`, which uses exponential backoff
(`utils.NewBackoff(50ms, 300ms, 2s)`) and retries only when `retryerrors.IsRetryableSQLError(err)`;
other errors become `backoff.Permanent`.

`SelectGatewayEnvelopesByTopics` in full (`pkg/db/sqlc/envelopes_v2.sql`):

```sql
WITH cursors AS (
 SELECT x.node_id AS cursor_node_id, y.seq_id AS cursor_sequence_id
 FROM unnest(@cursor_node_ids::INT[]) WITH ORDINALITY AS x(node_id, ord)
 JOIN unnest(@cursor_sequence_ids::BIGINT[]) WITH ORDINALITY AS y(seq_id, ord) USING (ord)
),
cursor_entries AS (
 SELECT t.topic, c.cursor_node_id AS node_id, c.cursor_sequence_id AS seq_id
 FROM unnest(@topics::BYTEA[]) AS t(topic)
 CROSS JOIN cursors AS c
),
filtered AS (
 SELECT sub.originator_node_id, sub.originator_sequence_id, sub.gateway_time, sub.topic
 FROM cursor_entries AS ce
 CROSS JOIN LATERAL (
  SELECT m.originator_node_id, m.originator_sequence_id, m.gateway_time, m.topic
  FROM gateway_envelopes_meta AS m
  WHERE m.topic = ce.topic
    AND m.originator_node_id = ce.node_id
    AND m.originator_sequence_id > ce.seq_id
  ORDER BY m.originator_sequence_id
  LIMIT @row_limit::INT
 ) AS sub
 ORDER BY sub.originator_node_id, sub.originator_sequence_id
 LIMIT @row_limit::INT
),
originator_ids AS ( SELECT DISTINCT originator_node_id FROM filtered )
SELECT bl.originator_node_id, bl.originator_sequence_id, bl.gateway_time, bl.topic, bl.originator_envelope
FROM originator_ids AS oi
CROSS JOIN LATERAL (
 SELECT f.originator_node_id, f.originator_sequence_id, f.gateway_time, f.topic, b.originator_envelope
 FROM filtered AS f
 JOIN gateway_envelopes_blob AS b
     ON b.originator_node_id = oi.originator_node_id
    AND b.originator_sequence_id = f.originator_sequence_id
 WHERE f.originator_node_id = oi.originator_node_id
) AS bl
ORDER BY bl.originator_node_id, bl.originator_sequence_id;
```

The shape matters: it is a **cross product of topics × originators**, one LATERAL index probe per pair,
using the covering index `gem_topic_orig_seq_idx (topic, originator_node_id, originator_sequence_id)
INCLUDE (gateway_time)`. With 10 000 topics and 100 cursor entries that is 1 000 000 probes — which is
exactly why `maxQueriesPerRequest` and `maxVectorClockLength` exist. **With a single sequence this
becomes one index range scan.**

#### Response

```protobuf
message QueryEnvelopesResponse { repeated OriginatorEnvelope envelopes = 1; }
```

Rows are unmarshalled one by one. **A row that fails to unmarshal is logged and skipped**, silently
shrinking the page.

There is a hard invariant check in the loop:

```go
if last, ok := lastSeen[nodeID]; ok && seqID < last {
    logger.Fatal("system invariant broken: unsorted envelope stream", ...)
}
```

`zap.Logger.Fatal` calls `os.Exit(1)` — **an out-of-order row from the database crashes the process.**

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Invalid filter (any `validateQuery` rule) | `InvalidArgument` | `invalid query: %w` | **verbatim** |
| Originator list lookup failed | `Internal` | `could not get originator list: %w` | `internal server error` |
| Query failed after retries | `Internal` | `could not select envelopes: %w` | `internal server error` |
| Out-of-order rows | *(process exit)* | `system invariant broken: unsorted envelope stream` | *(no response; the process exits)* |

#### Limits

| Limit | Value |
| --- | --- |
| Default page size | 1000 (`maxRequestedRows`) |
| Max page size | 1000; larger values silently clamped |
| Max topics + originators | 10 000 (`maxQueriesPerRequest`) |
| Max topic length | 128 bytes (`maxTopicLength`) |
| Max cursor entries | 100 (`maxVectorClockLength`) |
| Response size | 25 MiB (transport); **not** chunked — a large page can exceed it and fail |

#### Notes

- **Pagination is caller-driven**: the response carries no cursor. The client must compute the next
  cursor from the returned envelopes' `(originator_node_id, originator_sequence_id)` maxima.
- **Sort order** is `(originator_node_id, originator_sequence_id)` ascending — *not* time order, and
  not a merge across originators.
- The one-page-per-call design means a query with a topic filter and 100 cursor entries always returns
  at most 1000 rows regardless of how many topics matched.

---

### 6.3 QueryApi.Subscribe (XIP-83)

`pkg/api/message/subscribe.go`, `Service.Subscribe`. Bidirectional stream. This is the newest and most
carefully built endpoint in the tree, and the one worth porting most faithfully. A dedicated companion
document covers the spec and the conformance analysis: see `xip-83.md`. This section covers the
mechanics as an endpoint.

#### Request (client → server, repeatable)

```protobuf
message SubscribeRequest {
  oneof version { V1 v1 = 1; }
  message V1 {
    oneof request { Mutate mutate = 1; Ping ping = 2; Pong pong = 3; }
    message Mutate {
      repeated Subscription adds         = 1;
      repeated bytes        removes      = 2;
      bool                  history_only = 3;
      uint64                mutate_id    = 4;
      message Subscription { bytes topic = 1; Cursor last_seen = 2; }
    }
  }
}
message Ping { uint64 nonce = 1; }
message Pong { uint64 nonce = 1; }
```

#### Response (server → client)

```protobuf
message SubscribeResponse {
  oneof version { V1 v1 = 1; }
  message V1 {
    oneof response {
      Envelopes       envelopes;
      Started         started;
      Ping            ping;
      Pong            pong;
      TopicsLive      topics_live;
      CatchupComplete catchup_complete;
    }
    message Envelopes { repeated OriginatorEnvelope envelopes = 1; uint64 mutate_id = 2; }
    message Started   { uint32 keepalive_interval_ms = 1; repeated Capability capabilities = 2; }
    message TopicsLive      { repeated bytes topics = 1; }
    message CatchupComplete { uint64 mutate_id = 1; }
    enum Capability { CAPABILITY_UNSPECIFIED = 0; }
  }
}
```

#### Protocol flow

1. On accept, the handler immediately sends `Started{keepalive_interval_ms}` where the value is
   `s.options.SendKeepAliveInterval` in milliseconds (default 30 s). `capabilities` is left empty.
2. The client sends `Mutate` frames to add and remove topics in place.
3. Each `Mutate` with at least one **effective** add starts a **catch-up wave**, replaying history from
   the provided per-topic cursors. All the wave's replay frames carry `mutate_id = <the Mutate's id>`.
4. Live tail frames carry `mutate_id = 0`.
5. A wave ends with, in this order: its last replay frames → one `TopicsLive` listing its topics → one
   `CatchupComplete` echoing its `mutate_id`.
6. A `Mutate` that starts no wave (removes only, or all adds are no-ops) is acked with an **immediate**
   `CatchupComplete`.
7. Liveness: the server sends `Ping{nonce}` when the response channel has been idle for the keepalive
   interval, and reaps the stream if the matching `Pong` does not arrive within another interval. Client
   `Ping`s are always answered with `Pong`.
8. On half-close the server stops pinging, drains in-flight waves, and closes with `OK`.

#### Validation

`handleMutate` fully validates before mutating any state, so a bad `Mutate` never half-applies.
Validation order:

1. `len(adds) > 0 && mutate_id == 0` → `InvalidArgument`.
2. `mutate_id != 0` and matches any in-flight wave → `InvalidArgument`. **Checked for every Mutate**,
   including removes-only.
3. `len(adds) > maxMutateAdds` (100 000, pre-dedup) → `ResourceExhausted`.
4. Sum of cursor entries across adds > `maxMutateCursorEntries` (1 000 000, pre-dedup) →
   `ResourceExhausted`.
5. Every `removes` topic must `topic.ParseTopic` → `InvalidArgument` `remove: %w`.
6. Per add, in list order:
   - `topic.ParseTopic` → `InvalidArgument` `add: %w`.
   - Duplicate within this Mutate → **skipped**; the **first** occurrence wins.
   - Already subscribed and not being removed in this same Mutate:
     - `history_only` → `InvalidArgument` `history_only add targets a topic already subscribed on this stream`;
     - otherwise → **silently a no-op** (no re-gate, no wave, no cursor reset). Replay requires
       remove + re-add.
   - `hasInflightHistoryOnly(cursorKey)` → `InvalidArgument` `add targets a topic with an in-flight
     history_only catch-up`.
   - Each cursor entry must fit the signed DB columns: `nodeID <= MaxInt32`, `seqID <= MaxInt64`, else
     `InvalidArgument` `cursor entry out of range (originator %d, sequence %d)`. The code comment
     explains this is not pedantry — an out-of-range value would be dropped by the query *and* stored in
     the live cursor, permanently killing that topic on the stream.
7. Projected live-set size `|(live \ removes) ∪ adds| > maxActiveSubscribeTopics` (1 000 000) →
   `ResourceExhausted`. Only for non-`history_only`.
8. `len(sess.waves) >= maxInflightSubscribeWaves` (256) and this Mutate would start a wave →
   `ResourceExhausted`.

Then apply: **removes first**, then adds. A topic in both is therefore reset.

#### Storage (read path)

Catch-up runs in a detached goroutine, `Service.runSubscribeCatchUp`:

1. `s.originatorList.GetOriginatorNodeIDs(ctx)` — TTL-cached, DB round trip on a miss. Deliberately off
   the writer goroutine so a slow DB cannot false-reap a healthy stream.
2. `s.fetchWaveCeilingsWithRetry(ctx, ceilingOriginators(known, provided))` →
   `SelectOriginatorCeilings`, `COALESCE(MAX(originator_sequence_id), 0)` per originator. This **pins
   the wave's replay ceiling at wave start**, so the scan terminates under sustained publishing. The
   ceiling set is the union of the cached originator list and every originator any provided cursor
   names, because the scan's ceiling join is INNER.
3. Build floors: each topic's provided cursor, then `db.FillMissingOriginators(filled, known)`.
   Flattened once into `SelectGatewayEnvelopesWaveScanParams` and reused for every page.
4. Page loop with `RowLimit: topicPageLimit` (500), advancing `(ScanNodeID, ScanSequenceID)` from the
   **last raw row** via a keyset `>` row-value comparison. Each page is emitted as a `catchUpBatch`.
   Retried with `utils.NewBackoff(50ms, 300ms, 2s)` per page.
5. Emits `catchUpBatch{done: true}`.

`SelectGatewayEnvelopesWaveScan` is **one merged keyset scan in `(originator, sequence)` order across
all of the wave's topics** — not per-topic bursts. Its in-file comment cites XIP-83 requirement 4.

#### The seam

`routeLive` splits each live batch: envelopes for a `topicGated` topic go to `bufferLive` (held,
counted against `pendingBytes`); the rest go through `advanceLive` (cursor dedupe) and out tagged `0`.

`handleCatchUp`, on the wave's `done` marker:

1. Send the final page, tagged with the wave id, after `envsOwnedByWave` drops topics removed or reset
   under a newer wave.
2. Fold every still-owned gated topic's `pending` buffer, flip each to `topicLive`, collect `wire` bytes.
3. `sort.SliceStable` the folded envelopes by `(OriginatorNodeID, OriginatorSequenceID)` — "the wave's
   replay must stay totally ordered per originator ACROSS its topics."
4. Send the folded envelopes tagged with the **wave's** `mutate_id`.
5. Send `TopicsLive(wire)`, then `CatchupComplete(w.mutateID)`.
6. `delete(sess.waves, b.wave)`.

So a live (`mutate_id = 0`) frame for a wave's topic is never delivered before that wave's
`CatchupComplete`, while other topics' live frames keep flowing throughout.

#### Concurrency model

**Single writer.** The `select` loop owns every piece of mutable state. Three pure producers feed it via
channels:

| Goroutine | Channel | Depth |
| --- | --- | --- |
| Sender (sole caller of `stream.Send`) | `sess.outbound` | `subscribeSendQueueDepth` = 8 |
| Frame reader (`stream.Receive`) | `requestCh` | 16 |
| Catch-up fetchers (one per wave) | `sess.catchUpCh` | `subscribeCatchUpQueueDepth` = 16 |

`streamCtx, cancel := context.WithCancel(ctx)` with `defer cancel()`, because "connect-go does NOT
cancel the stream context when the handler returns."

`sess.send` is bounded by a reused `sendTimer` of `keepAlive`: a wedged sender fails the stream with
`Unavailable: send stalled; client not reading` rather than parking the writer forever.

#### Liveness, precisely

Two **independent** timers:

- `pingTicker` — the send-idle cadence. `sess.send` calls `pingTicker.Reset(keepAlive)` on every frame
  actually enqueued.
- `pongDeadline` — the reap deadline. Armed **only** when a Ping is sent, disarmed **only** by a Pong
  whose nonce equals `sess.pingNonce` (the latest).

The code comment states why: "(A single shared ticker let ordinary delivery keep resetting the reap,
defeating silent-death detection.)"

Before reaping, `drainPendingRequests` non-blockingly processes queued frames, "so a Pong that landed
in the buffer just as the deadline fired (Go select picks randomly among ready cases) is still counted,
instead of false-reaping a stream that did answer."

Pings are suppressed while `halfClosed`.

#### Framing

`sess.sendEnvelopes(envs, mutateID)` splits into frames under `maxSubscribeFrameBytes` (2 MiB), an order
of magnitude below the 25 MiB transport cap. An envelope larger than 2 MiB goes out alone. An envelope
whose **framed** size exceeds `constants.GRPCPayloadLimit` is **skipped with a warning** — the comment
argues the alternative is worse, since "a reconnecting client's wave would hit the same row again,
wedging it permanently."

#### Errors

| Condition | Code returned by `Subscribe` | Message the handler builds | Wire message |
| --- | --- | --- | --- |
| No `V1` arm | `InvalidArgument` | `unrecognized SubscribeRequest version` | **verbatim** |
| Adds with `mutate_id == 0` | `InvalidArgument` | `a Mutate with adds requires a nonzero mutate_id` | **verbatim** |
| `mutate_id` collides with in-flight wave | `InvalidArgument` | `mutate_id %d is already in flight on this stream` | **verbatim** |
| Unparseable remove topic | `InvalidArgument` | `remove: %w` | **verbatim** |
| Unparseable add topic | `InvalidArgument` | `add: %w` | **verbatim** |
| `history_only` on subscribed topic | `InvalidArgument` | `history_only add targets a topic already subscribed on this stream` | **verbatim** |
| Add on topic with in-flight `history_only` | `InvalidArgument` | `add targets a topic with an in-flight history_only catch-up` | **verbatim** |
| Cursor entry out of range | `InvalidArgument` | `cursor entry out of range (originator %d, sequence %d)` | **verbatim** |
| Adds cap | `ResourceExhausted` | `adds per Mutate limit %d exceeded; split adds across multiple Mutates` | `request has failed` |
| Cursor entries cap | `ResourceExhausted` | `cursor entries per Mutate limit %d exceeded; split adds across multiple Mutates` | `request has failed` |
| Active topic cap | `ResourceExhausted` | `active topic limit %d exceeded` | `request has failed` |
| In-flight wave cap | `ResourceExhausted` | `in-flight catch-up limit %d exceeded` | `request has failed` |
| Pending buffer overflow | `ResourceExhausted` | `pending buffer exceeded while catching up` | `request has failed` |
| Worker reaped the listener | `Aborted` | `subscription closed: consumer too slow` | `request has failed` |
| Catch-up fetch failed | `Unavailable` | `catch-up failed: %w` | `request has failed` |
| **Ceiling query failed after retries** | **`Unavailable`** — see below | `catch-up failed: could not select originator ceilings: %w` | `request has failed` |
| **Wave scan page failed after retries** | **`Unavailable`** — see below | `catch-up failed: could not select envelopes: %w` | `request has failed` |
| `stream.Receive` failed (non-EOF) | `Unavailable` | `stream recv failed: %w` | `request has failed` |
| Send blocked past keepalive | `Unavailable` | `send stalled; client not reading` | `request has failed` |
| Service shutting down | `Unavailable` | `service is shutting down` | `request has failed` |
| No Pong within deadline | `DeadlineExceeded` | `no Pong within deadline` | `request has failed` |
| Flush timed out | `DeadlineExceeded` | `flush timed out waiting for sender to drain` | `request has failed` |
| Flush interrupted | `Canceled` | `flush interrupted before drain completed: %w` | `request was canceled` |

**Why the two catch-up query rows are `Unavailable`, not `Internal`.** The helpers
`fetchWaveCeilingsWithRetry` and `fetchWaveScanPageWithRetry`
(`pkg/api/message/subscribe.go`) each build a `connect.CodeInternal` error after their backoff is
exhausted. But those helpers run on the **fetcher goroutine**, not the writer, and they do not return
to the client. `runSubscribeCatchUp` puts the error into `catchUpBatch.err`, and
`handleCatchUp` re-wraps whatever it finds there:

```go
if b.err != nil {
    // A fetch error: fail so the client reconnects from its cursors, rather than emit a
    // misleading CatchupComplete over a history gap.
    return false, connect.NewError(
        connect.CodeUnavailable,
        fmt.Errorf("catch-up failed: %w", b.err),
    )
}
```

So the `Internal` code is **discarded** and the stream fails with `Unavailable`. Its message is then
rewritten to `request has failed` (§6.0), meaning a client cannot tell a ceiling failure from a scan
failure from a generic catch-up failure — it sees only `Unavailable`. That is the intended signal:
`Unavailable` tells the client to reconnect from its durable cursors, which is exactly right here.

**A note on topic size**: this endpoint enforces **no maximum topic length**. `handleMutate` calls
`topic.ParseTopic` on every add and remove, which rejects only `< 2` bytes and unknown kinds
(`pkg/topic/topic.go`, `ParseTopic`). The legacy `maxTopicLength = 128` check applied by
`validateQuery` and `validateTopicFilter` is **not** applied on the XIP-83 path. The only bound on
topic size here is the 25 MiB request-frame cap. See §6.3's limits table.

#### Limits

| Limit | Value |
| --- | --- |
| `maxActiveSubscribeTopics` | 1 000 000 |
| `maxMutateAdds` | 100 000 (pre-dedup) |
| `maxMutateCursorEntries` | 1 000 000 (pre-dedup) |
| `maxInflightSubscribeWaves` | 256 |
| `maxSubscribePendingBytes` | 64 MiB |
| `maxSubscribeFrameBytes` | 2 MiB |
| Catch-up page size | 500 (`topicPageLimit`) |
| Live listener channel depth | 1024 (`subscriptionBufferSize`) |
| Keepalive / ping interval | `SendKeepAliveInterval`, default 30 s |
| Max topic length | **none** — `ParseTopic` checks a 2-byte minimum and the kind byte only; `maxTopicLength` (128) is **not** applied here |
| Mutation rate | **unlimited** |
| Client Ping rate | **unlimited** |
| Per-RPC admission (open / rate limit) | **none** — see §13.2 |

#### Notes

- **An empty `V1` request oneof is silently ignored.** `handleRequest` rejects a request with **no
  `V1` arm** (`InvalidArgument`, `unrecognized SubscribeRequest version`), but a request that *has* a
  `V1` and sets none of `mutate` / `ping` / `pong` falls through the `switch` to `default: return nil`
  (`pkg/api/message/subscribe.go`, `handleRequest`). No response, no error, no ack — the frame is a
  no-op. A client that sent a malformed Mutate this way waits forever for a `CatchupComplete` that
  never comes. **[FIX]** — reject an empty V1 oneof rather than dropping it.
- **Backpressure policy is "drop the subscription."** When the worker's 1024-deep listener channel
  fills, `closeListener` closes it, and the handler returns `Aborted`. The client is expected to
  reconnect from durable cursors.
- **No per-topic authorization.** Any client can subscribe to any topic.
- Documented gap: a `history_only` wave misses originators that neither the TTL-cached list nor the
  client's cursor names. Live waves are covered by the gate; bounded sync is not. The code calls this
  "an accepted eventual-consistency property of history_only."

---

### 6.4 ReplicationApi/QueryApi.SubscribeTopics

`pkg/api/message/subscribe_topics.go`, `Service.SubscribeTopics`. The immutable server-streaming
ancestor of `Subscribe`. Kept for browser / gRPC-web clients that cannot open a bidi stream.

#### Request

```protobuf
message SubscribeTopicsRequest {
  repeated TopicFilter filters = 1;
  message TopicFilter { bytes topic = 1; Cursor last_seen = 2; }
}
```

#### Response

```protobuf
message SubscribeTopicsResponse {
  oneof response {
    Envelopes    envelopes     = 1;
    StatusUpdate status_update = 2;
  }
  message Envelopes { repeated OriginatorEnvelope envelopes = 1; }
  message StatusUpdate { SubscriptionStatus status = 1; }
  enum SubscriptionStatus {
    SUBSCRIPTION_STATUS_UNSPECIFIED      = 0;
    SUBSCRIPTION_STATUS_STARTED          = 1;
    SUBSCRIPTION_STATUS_CATCHUP_COMPLETE = 2;
    SUBSCRIPTION_STATUS_WAITING          = 3;
  }
}
```

#### Flow

1. **Rate-limit admission** (`applySubscribeAdmission`) — charged *before* anything else, cost
   `ceil(sqrt(max(numFilters,1)))` (§13).
2. Send `STARTED` immediately, "so wasm-based clients maintain the connection open."
3. `validateTopicFilters`.
4. `s.originatorList.GetOriginatorNodeIDs(ctx)`.
5. `buildTopicCursors(filters, knownOriginators)` — dedupes topics (first filter wins), copies each
   `last_seen` map, and `db.FillMissingOriginators` on each. A nil `last_seen` means "from the
   beginning."
6. Register a live listener with the subscribe worker **before** catch-up: `s.subscribeWorker.listen`.
7. `s.catchUpTopics(...)` — see below.
8. Send `CATCHUP_COMPLETE`.
9. Loop: forward live batches through `advanceTopicCursors` (dedupe + advance), or send `WAITING` on the
   keepalive ticker.

#### Catch-up algorithm

`Service.catchUpTopics` chunks the topic keys into groups of `maxTopicsPerChunk` (500). For each chunk:

- `rowsPerEntry := db.CalculateRowsPerEntry(len(chunkKeys), topicPageLimit)` = `max(500/n, 10)`.
- Loop `SelectGatewayEnvelopesByPerTopicCursors` with `RowsPerEntry` and `RowLimit = 500`.
- `advanceTopicCursors(cursors, envs, logger)` dedupes against and advances the shared cursors in place
  — which is what drives the *next* page.
- Break when `int32(len(rows)) < rowsPerEntry`. The comment explains why it compares against
  `rowsPerEntry` and not `topicPageLimit`: "the LATERAL query distributes topicPageLimit across (topic,
  originator) pairs via per-entry sub-limits, so total rows returned can be less than topicPageLimit
  even when more data exists."

#### Validation

`validateTopicFilters` / `validateTopicFilter`:

| Rule | Code | Message |
| --- | --- | --- |
| `len(filters) == 0` | `InvalidArgument` | `filters must not be empty` |
| `len(filters) > maxTopicFilters` (10 000) | `InvalidArgument` | `too many filters: %d, maximum is %d` |
| topic empty or > 128 bytes | `InvalidArgument` | `invalid topic length: %d` |
| cursor entries > 100 | `InvalidArgument` | `vector clock length exceeds maximum of %d` |

The doc comment is explicit that **unknown originators in a cursor are allowed**: "a client may learn of
a new originator before this node has indexed any of its messages, or may still hold cursors for
originators removed long ago. Unknown originators are harmless in the downstream LATERAL query — they
simply match no rows."

Note: topics are **not** parsed here either. `newListener` does parse them
(`pkg/api/message/listener.go`) and *skips* unparseable ones with a warning — so an invalid topic
passes validation, is accepted, and then silently never delivers.

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Rate limit | `ResourceExhausted` | `subscribe admission rate limit exceeded` | `request has failed` |
| Any filter rule | `InvalidArgument` | see table above | **verbatim** |
| Could not send STARTED / CATCHUP_COMPLETE / WAITING | `Internal` | `could not send status: %w` / `could not send keepalive: %w` | `internal server error` |
| Originator list failed | `Internal` | `could not get originator list: %w` | `internal server error` |
| Catch-up query failed | `Internal` | `could not select envelopes: %w` | `internal server error` |
| Send envelopes failed | `Internal` | `error sending envelopes: %w` | `internal server error` |

#### Limits

| Limit | Value |
| --- | --- |
| Max filters | 10 000 |
| Max topic length | 128 bytes |
| Max cursor entries per filter | 100 |
| Catch-up chunk | 500 topics |
| Catch-up page | 500 rows, min 10 per (topic, originator) |
| Keepalive | `SendKeepAliveInterval`, default 30 s |
| Live listener channel | 1024 |
| Frame size | **not split** — one `stream.Send` per live batch; can exceed 25 MiB and abort |

#### Notes

- **No duplicate suppression across a reconnect** beyond what the client's own cursor provides.
- Ordering within the stream is only what the worker's dispatch order gives: per originator ascending,
  no cross-topic merge guarantee during live tail.
- Compared to `Subscribe`: no mutation, no ping/pong (only a one-way `WAITING` heartbeat), no per-frame
  wave tag, per-topic catch-up bursts rather than a merged scan.

---

### 6.5 ReplicationApi.SubscribeEnvelopes

`pkg/api/message/service.go`, `Service.SubscribeEnvelopes`. The oldest subscribe form. Uses a single
`EnvelopesQuery` (topics **or** originators, plus one shared cursor) instead of per-topic cursors.

#### Request / Response

```protobuf
message SubscribeEnvelopesRequest  { EnvelopesQuery query = 1; }
message SubscribeEnvelopesResponse { repeated OriginatorEnvelope envelopes = 1; }
```

An **empty response message is the keepalive** — there is no status enum.

#### Flow (`Service.doSubscribe`)

1. Send an empty response immediately as a keepalive, "so wasm based clients maintain the connection
   open."
2. `s.subscribeWorker.listen(ctx, query)`.
3. `s.catchUpFromCursor` → `catchUpWithSendFn`, which loops `fetchEnvelopesWithRetry` at
   `maxRequestedRows` (1000) per page, sleeping `pagingInterval` (100 ms) between pages, until a short
   page.
4. Loop: forward live batches, or send an empty keepalive on the ticker.

Catch-up mode is `catchUpFromCursor` only when `query.last_seen != nil`; otherwise `catchUpNone` and the
stream starts at the live tail.

#### Batching

`Service.sendEnvelopes` → `batchAndSendEnvelopes` (`pkg/api/message/subscribe_common.go`), which is the
size-aware batcher the newer paths replaced:

- Skips envelopes already at or below `cursor[origID]`.
- Computes exact wire size: `proto.Size(env) + envelopeOverhead(size)`, where `envelopeOverhead`
  computes `1 byte tag + varint length` from first principles.
- Flushes when adding the next envelope would exceed `constants.GRPCPayloadLimit` minus the wrapper
  overhead (0 for `SubscribeEnvelopesResponse`, 10 for `SubscribeOriginatorsResponse`).
- An envelope that alone exceeds the limit is **logged and skipped**, and the cursor is advanced past it
  so pagination does not stick.

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Invalid filter | `InvalidArgument` | `invalid subscription request: %w` | **verbatim** |
| Keepalive send failed | `Internal` | `could not send keepalive: %w` | `internal server error` |
| Envelope send failed | `Internal` | `error sending envelope: %w` / `error sending envelopes: %w` | `internal server error` |
| Catch-up query failed | `Internal` | `could not select envelopes: %w` | `internal server error` |

Note `validateQuery(query, allowEmpty=false)` here, so `query must contain either topics or originators`
is an error for subscribe but not for query.

#### Limits

Same `maxQueriesPerRequest` / `maxTopicLength` / `maxVectorClockLength` as `QueryEnvelopes`; catch-up
page 1000; live channel 1024; keepalive `SendKeepAliveInterval`.

---

### 6.6 ReplicationApi.SubscribeOriginators

`pkg/api/message/subscribe_originators.go`, `Service.SubscribeOriginators`. **[DROP]** — a
node-to-node replication stream, meaningless without originators.

#### Request

```protobuf
message SubscribeOriginatorsRequest {
  OriginatorFilter filter = 1;
  message OriginatorFilter { repeated uint32 originator_node_ids = 1; Cursor last_seen = 2; }
}
```

#### Validation

`validateOriginatorFilter`, all → `InvalidArgument`:

| Rule | Message |
| --- | --- |
| `filter == nil` | `filter must not be nil` |
| no originator ids | `filter must contain at least one originator node id` |
| `last_seen == nil` | `last_seen cursor is required` |

Then `validateQuery(query, allowEmpty=false)`.

Unlike `SubscribeEnvelopes`, the cursor is **mandatory** — this stream always catches up.

#### Response

`SubscribeOriginatorsResponse` wraps envelopes in a nested oneof, costing
`originatorResponseOverhead = 10` bytes per batch, which `batchAndSendEnvelopes` accounts for.

#### Errors

Same shape as `SubscribeEnvelopes` plus the filter rules above.

---

### 6.7 NotificationApi.SubscribeAllEnvelopes

`pkg/api/message/service.go`, `Service.SubscribeAllEnvelopes`. A firehose: **no filter at all**.

#### Request / Response

```protobuf
message SubscribeAllEnvelopesRequest {}   // no fields used
// response type is SubscribeEnvelopesResponse
```

The handler builds `&subscribeFilter{catchUpMode: catchUpNone, cursor: make(map[uint32]uint64)}` and
calls the same `doSubscribe`. An empty filter makes `newListener` set `isGlobal = true`
(`pkg/api/message/listener.go`), registering in `globalListeners` and receiving **every** envelope the
worker sees.

There is **no catch-up** — it starts at the live tail.

#### Limits and protection

This is the one endpoint with a **concurrent stream limit**: the `NotificationApi` handler group gets
`StreamLimitInterceptor` (`pkg/interceptors/server/stream_limit.go`), enforcing
`T1MaxConcurrentSubscribeAll` (default **2**) concurrent streams per client IP, via a Redis counter with
a `StreamTTL` (default 15 min) self-heal window refreshed every `StreamRefreshInterval` (default 5 min).

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Too many concurrent streams | `ResourceExhausted` | `concurrent stream limit exceeded` | `request has failed` |
| Limiter error | — | **fails open**, logged `stream limiter error, allowing stream` | *(no error returned)* |

**[KEEP the idea]** — a firehose is genuinely useful for a self-hosted backend (bridges, indexers,
backups), and it needs exactly this kind of concurrency cap.

---

### 6.8 ReplicationApi/QueryApi.GetNewestEnvelope

`pkg/api/message/service.go`, `Service.GetNewestEnvelope`.

#### Request / Response

```protobuf
message GetNewestEnvelopeRequest  { repeated bytes topics = 1; }
message GetNewestEnvelopeResponse {
  repeated Response results = 1;
  message Response { OriginatorEnvelope originator_envelope = 1; }
}
```

The response is **positional**: `results[i]` corresponds to `topics[i]`. A topic with no envelope leaves
a `nil` entry. The handler builds `originalSort map[string]int` from the request order and scatters rows
back into place.

**The positional guarantee holds only when the requested topics are unique.** `originalSort` is a
map keyed by topic bytes, filled in request order:

```go
for idx, topic := range topics {
    originalSort[string(topic)] = idx
}
```

A repeated topic **overwrites** its earlier index, so only the **last** occurrence survives. The
scatter loop then runs once per returned row — and `SelectNewestFromTopics` uses
`DISTINCT ON (m.topic)`, so it returns **one** row per distinct topic no matter how many times the
caller listed it. The result: with `topics = [A, B, A]`, `results` has length 3, slot 2 holds A's
envelope, and **slot 0 is left `nil`** even though topic A does have an envelope. The caller sees a
false "no data for this topic" at every duplicate slot but the last.

There is no validation to prevent this — the endpoint does not deduplicate, count, or reject
duplicates. **[FIX]** — either deduplicate the request and document the mapping, or scatter into
every matching index rather than one.

#### Validation

**None.** No length check, no count check, no topic parsing, no rate limiting beyond the shared
`QueryApi` interceptor. This is the least-guarded endpoint in the tree.

#### Storage

`SelectNewestFromTopics` (`pkg/db/sqlc/envelopes_v2.sql`):

```sql
SELECT DISTINCT ON (m.topic) ...
FROM gateway_envelopes_meta m JOIN gateway_envelopes_blob b USING (originator_node_id, originator_sequence_id)
WHERE m.topic = ANY(@topics::BYTEA[])
ORDER BY m.topic, m.gateway_time DESC
```

Uses `gem_topic_time_desc_idx (topic, gateway_time DESC)`.

**"Newest" means the largest `gateway_time`, not the largest sequence id.** The query file carries an
explicit TODO: *"sorting by gateway time can lead to wrong results, this query needs to be redone."*
`gateway_time` is a `TIMESTAMP` set by the writing node's clock, so across originators it is not a
reliable order, and two rows in the same microsecond tie arbitrarily.

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Query failed | `Internal` | `could not select envelopes: %w` | `internal server error` |
| Row fails to unmarshal | — | logged and skipped, leaving a nil result | *(no error returned)* |

#### Notes

**[KEEP, but fix]** — the endpoint is useful (it answers "what is the latest state of these
conversations?" in one round trip). With one sequence, `DISTINCT ON (topic) ... ORDER BY topic,
sequence_id DESC` is both correct and cheaper. Add a topic-count limit while you are there.

---

### 6.9 ReplicationApi/QueryApi.GetInboxIds

`pkg/api/message/service.go`, `Service.GetInboxIds`.

#### Request / Response

```protobuf
message GetInboxIdsRequest {
  repeated Request requests = 1;
  message Request { string identifier = 1; /* identifier_kind also present */ }
}
message GetInboxIdsResponse {
  repeated Response responses = 1;
  message Response { string identifier = 1; optional string inbox_id = 2; }
}
```

Also positional: `responses[i]` matches `requests[i]`, with `inbox_id` left unset when no association
exists.

#### Validation

Only `len(requests) > maxInboxIdsPerRequest` (1000) → `InvalidArgument`, `too many requests`.

`identifier_kind` is present in the proto but **ignored** by the handler; only `GetIdentifier()` is
read. Addresses are used verbatim as the SQL parameter — there is no normalization or checksum
handling at this layer. (`GetPayerByAddress` elsewhere does `common.HexToAddress(address).Hex()`, but
`GetInboxIds` does not.) **[UNVERIFIED]** whether callers are expected to pre-normalize.

#### Storage

`GetAddressLogs` (`pkg/db/sqlc/identity_updates.sql`):

```sql
SELECT a.address, encode(a.inbox_id, 'hex') AS inbox_id, a.association_sequence_id
FROM address_log a
INNER JOIN (
    SELECT address, MAX(association_sequence_id) AS max_association_sequence_id
    FROM address_log
    WHERE address = ANY (@addresses::TEXT[]) AND revocation_sequence_id IS NULL
    GROUP BY address
) b ON a.address = b.address AND a.association_sequence_id = b.max_association_sequence_id;
```

"The current inbox for an address" = the row with the **highest** `association_sequence_id` among rows
whose `revocation_sequence_id IS NULL`. The `address_log` table is a **materialized projection** built
by the identity-update indexer (§10), not derived on the fly from envelopes.

The result scatter is an **O(addresses × rows) nested loop** in Go:

```go
for index, address := range addresses {
    for _, logEntry := range addressLogEntries {
        if logEntry.Address == address { ... }
    }
}
```

At the 1000-address cap that is up to a million string comparisons. Trivially a map lookup.

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| > 1000 requests | `InvalidArgument` | `too many requests` | **verbatim** |
| DB error | *(bare error)* | returned raw | `unknown error` (code `Unknown`) |

#### Notes

**[KEEP]** — address → inbox-id resolution is core protocol functionality independent of the
blockchain. The projection table and the monotonic-sequence upsert guards are the right shape; only the
source of `association_sequence_id` changes (§10).

---

### 6.10 PublishApi.PublishPayerEnvelopes

Identical handler to §6.1 (`Service` implements `PublishApiHandler` too). The only difference is the
interceptor chain: `PublishApi` handlers get the base `handlerOpts` — no rate limiter, no
`RequireNodeAuthInterceptor` — whereas `ReplicationApi` can be gated behind node auth via
`--api.require-replication-node-auth`.

Practical reading: `ReplicationApi` is the node-to-node/payer-to-node surface that can be locked down;
`PublishApi` is the same thing left open. **[UNVERIFIED]** why both exist; likely a migration artifact.

---

### 6.11 PayerApi/GatewayApi.PublishClientEnvelopes

`pkg/api/payer/service.go`, `Service.PublishClientEnvelopes`. **[DROP]** in its entirety — but read it
first, because it is where the *client-visible* publish semantics live, and the new backend has to
absorb them.

#### Request / Response

```protobuf
message PublishClientEnvelopesRequest  { repeated ClientEnvelope envelopes = 1; }
message PublishClientEnvelopesResponse { repeated OriginatorEnvelope originator_envelopes = 1; }
```

The response is positional: `originator_envelopes[i]` corresponds to `envelopes[i]`, reassembled from
two very different code paths via `clientEnvelopeWithIndex.originalIndex`.

#### Routing (`Service.groupEnvelopes`)

For each client envelope:

1. `envelopes.NewClientEnvelope(raw)` → `InvalidArgument` `invalid client envelope at index %d: %w`.
2. `clientEnvelope.TopicMatchesPayload()` → `InvalidArgument` `client envelope at index %d does not
   match topic`.
3. `shouldSendToBlockchain(clientEnvelope)`:
   - `TopicKindIdentityUpdatesV1` → **always** chain;
   - `TopicKindGroupMessagesV1` → chain iff `deserializer.ShouldSendToBlockchain` says commit/proposal;
   - anything else → node.
4. Node-bound envelopes: `s.nodeSelector.GetNode(clientEnvelope.TargetTopic())` picks the originator.
   Default strategy is `stable` (`selectors.NewStableHashingNodeSelectorAlgorithm`) — a consistent hash
   of the topic, so **all writes to one topic land on one node**, which is what makes per-topic
   ordering possible at all in the decentralized model.

#### Node path

`Service.publishToNodeWithRetry`:

- `retryCount = cmp.Or(s.cfg.PublishRetries, 1)` — default 5 from `--payer.envelope-publish-retries`.
- Each attempt gets `context.WithTimeout(ctx, s.cfg.PublishTimeout)` — default 30 s.
- On failure the node is added to a **banlist** and `nodeSelector.GetNode(topic, banlist)` picks
  another.
- A client-cancelled context aborts; a *deadline exceeded* is "treated as the nodes fault" and retried.
- `Service.publishToNode` then calls `signAllClientEnvelopes` → `signClientEnvelope`, which builds the
  `PayerEnvelope` with the payer signature, `TargetOriginator`, and `MessageRetentionDays` from
  `determineRetentionPolicy` (always 60), and forwards to that node's
  `ReplicationApi.PublishPayerEnvelopes`.

#### Blockchain path

`Service.publishToBlockchain`, by topic kind:

| Kind | Contract call | Synthetic originator |
| --- | --- | --- |
| `TopicKindGroupMessagesV1` | `blockchainPublisher.PublishGroupMessage(ctx, groupID, payload)` | `constants.GroupMessageOriginatorID` = 0 |
| `TopicKindIdentityUpdatesV1` | `blockchainPublisher.PublishIdentityUpdate(ctx, inboxID, payload)` | `constants.IdentityUpdateOriginatorID` = 1 |
| anything else | — | `InvalidArgument` `unknown blockchain message for topic %s` |

The contract emits an event carrying a **`SequenceId` assigned by the contract**. The payer synthesizes
an `OriginatorEnvelope` from it (`buildUnsignedOriginatorEnvelopeFromChain`) with:

- `OriginatorNodeId` = 0 or 1,
- `OriginatorSequenceId` = the contract's sequence id,
- `OriginatorNs` = `time.Now().UnixNano()` with a `// TODO: get this data from the chain`,
- a `PayerEnvelope` containing **only** `UnsignedClientEnvelope` — no signature, no retention,
- `Proof = BlockchainProof{TransactionHash}`.

**This is the crux of xmtpd's global ordering, and §10 explains why.**

#### Size limit

If `s.maxPayerMessageSize != 0`, each **blockchain-bound** envelope's serialized size is checked:

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Envelope too large | `InvalidArgument` | `message at index %d too large` | **verbatim** |

Default from `--app-chain.max-blockchain-payload-size` = **200 000 bytes**. Node-bound envelopes are
not checked here — only the 25 MiB transport cap applies.

#### Errors

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Grouping/validation failure | `InvalidArgument` | `error grouping envelopes: %w` | **verbatim** |
| Blockchain payload too large | `InvalidArgument` | `message at index %d too large` | **verbatim** |
| Client cancelled | `Canceled` | `request canceled by client` | `request was canceled` |
| Node publish failed after all retries | `Internal` | `error publishing payer envelopes: %w` | `internal server error` |
| Blockchain publish failed | `Internal` | `error publishing group message: %w` | `internal server error` |
| Node selection failed | `InvalidArgument` | `error getting node for topic: %w` | **verbatim** |
| No client connection | `Internal` | `error getting client: %w` | `internal server error` |
| Signing failed | `Internal` | `error signing payer envelopes: %w` | `internal server error` |

---

### 6.12 PayerApi/GatewayApi.GetNodes

`pkg/api/payer/service.go`, `Service.GetNodes`. **[DROP]**

```protobuf
message GetNodesRequest  {}
message GetNodesResponse { map<uint32, string> nodes = 1; }  // node id -> HTTP address
```

Reads `s.nodeRegistry.GetNodes()` (backed by the on-chain node registry contract, refreshed every
`--settlement-chain.node-registry-refresh-interval`, default 60 s). Emits
`xmtp_gateway_get_nodes_available_nodes`.

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Registry read failed | `Internal` | `failed to fetch nodes: %w` | `internal server error` |

No validation, no limits, no auth.

---

### 6.13 MetadataApi.GetSyncCursor

`pkg/api/metadata/service.go`, `Service.GetSyncCursor`.

```protobuf
message GetSyncCursorRequest  {}
message GetSyncCursorResponse { Cursor latest_sync = 1; }
```

Returns `s.cu.GetCursor()` — the in-memory vector clock maintained by `DBBasedCursorUpdater`
(`pkg/api/metadata/cursor_updater.go`), which polls `SelectVectorClock` **every 100 ms** and stores
`map[originator_node_id]max_sequence_id` under an `RWMutex`.

**One database read error stops the updater permanently.** The poll loop
(`DBBasedCursorUpdater.start`) is:

```go
case <-ticker.C:
    updated, err := cu.read()
    if err != nil {
        // TODO proper error handling
        return
    }
```

The `return` exits the goroutine. There is no retry, no backoff, no restart, and no metric or alarm —
only a `TODO`. After the first transient read failure (a statement timeout, a replica blip, a
connection reset) the ticker stops and the cached cursor is **frozen at its last successful value**
for the lifetime of the process.

The blast radius is wider than this endpoint. The same `cu` object backs:

- `MetadataApi.GetSyncCursor` and `SubscribeSyncCursor` — clients are told the node is stuck at an old
  sequence and never learn otherwise. `SubscribeSyncCursor` keeps sending its 30 s keepalive with the
  stale value, so the stream looks healthy.
- `Service.validateClientInfo`'s `depends_on` checks (§2.3) — which read `s.cu.GetCursor()`. A frozen
  cursor makes those checks reject `depends_on` references to any originator or sequence that
  advanced after the failure, with `InvalidArgument`, `node ID %d ... has not been seen by this node`
  or `sequence ID %d ... exceeds last seen sequence ID %d`. Valid publishes start failing and the
  message points at the client.

Only a process restart recovers it. **[FIX]** — retry with backoff, keep polling, and export a
staleness metric. This is one of the cheapest reliability fixes in the tree.

No validation, no limits, no errors other than transport. The returned map is the *live* map, not a
copy — `GetCursor` returns `&envelopes.Cursor{NodeIdToSequenceId: cu.cursor}` while holding only a read
lock, so the caller shares the map with the updater. **[UNVERIFIED]** whether any caller mutates it;
`read()` replaces the map wholesale rather than mutating in place, which makes this safe in practice.

**[KEEP the concept]** — "what is the newest thing you have?" is the single most useful metadata call
for a client deciding whether it is caught up. With one sequence it becomes a scalar.

---

### 6.14 MetadataApi.SubscribeSyncCursor

`pkg/api/metadata/service.go`, `Service.SubscribeSyncCursor`. Server stream of
`GetSyncCursorResponse`.

Flow:

1. Send the current cursor immediately.
2. Register an update channel under a synthetic client id
   (`fmt.Sprintf("client-%d", time.Now().UnixNano())`) with `cu.AddSubscriber`; `defer
   cu.RemoveSubscriber`.
3. Loop: on notification send the cursor; on a **30 s** keepalive ticker
   (`subscribeSyncCursorKeepaliveInterval`) send it again regardless.

The updater only notifies when the cursor actually **changed** (`equalCursors` comparison in
`DBBasedCursorUpdater.read`), and `notifySubscribers` uses a non-blocking send into a depth-1 channel —
so a slow consumer coalesces updates rather than blocking the updater.

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Send failed | `Internal` | `error sending cursor: %w` | `internal server error` |
| Keepalive send failed | `Internal` | `error sending keepalive cursor: %w` | `internal server error` |

The keepalive comment is worth quoting: "Without it, any intermediary (LB, reverse proxy) with an idle
timeout will terminate the HTTP/2 stream and force clients to reconnect." This is the same lesson
XIP-83 formalizes.

---

### 6.15 MetadataApi.GetVersion

```protobuf
message GetVersionRequest  {}
message GetVersionResponse { string version = 1; }
```

Returns `s.version.String()` (a `*semver.Version` injected at construction).

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| `s.version == nil` | `Internal` | `version is not set` | `internal server error` |

---

### 6.16 MetadataApi.GetPayerInfo

`pkg/api/metadata/service.go`, `Service.GetPayerInfo`. **[DROP]**

```protobuf
message GetPayerInfoRequest {
  repeated string payer_addresses = 1;
  PayerInfoGranularity granularity = 2;   // UNSPECIFIED | HOUR | DAY
}
message GetPayerInfoResponse {
  map<string, PayerInfo> payer_info = 1;
  message PayerInfo { repeated PeriodSummary period_summaries = 1; }
  message PeriodSummary {
    uint64 amount_spent_picodollars = 1;
    uint64 num_messages             = 2;
    uint64 period_start_unix_seconds = 3;
  }
}
```

Granularity defaults to **hour** when unspecified. For each address: `GetPayerByAddress` (which *does*
normalize via `common.HexToAddress(address).Hex()`), then `GetPayerInfoReport` which buckets
`unsettled_usage` by `DATE_TRUNC(@group_by, ...)`.

| Condition | Code | Message | Wire message |
| --- | --- | --- | --- |
| Empty `payer_addresses` | `InvalidArgument` | `payer_addresses cannot be empty` | **verbatim** |
| Address not found (`sql.ErrNoRows`) | `NotFound` | `payer address not found: %s` | **verbatim** (`NotFound` is preserved) |
| Payer lookup failed | `Internal` | `failed to look up payer` | `internal server error` |
| Report query failed | `Internal` | `failed to get payer info for address: %s` | `internal server error` |

**No limit on the number of addresses** — each triggers two sequential queries, so N addresses cost 2N
round trips.

---

### 6.17 MisbehaviorApi (not served)

`pkg/proto/xmtpv4/message_api/misbehavior_api*.pb.go` defines `SubmitMisbehaviorReport` and
`QueryMisbehaviorReports`, and `pkg/misbehavior/*.go` implements report construction and verification —
but **no handler is registered** in `pkg/server/server.go`. The service is dead code at the RPC layer.
**[DROP]**

---

## 7. Ordering and sequence ids

This is the most important section for the new backend. Read it before anything else.

### 7.1 There is only one sequence concept

Despite the naming in some documentation, **there is no `gateway_sequence_id` column anywhere in the
schema**. Confirmed by grep across `pkg/`. There is exactly one:

- `staged_originator_envelopes.id` — a `BIGSERIAL`, assigned at staging;
- copied **verbatim** into `gateway_envelopes_meta.originator_sequence_id` at publish;
- surfaced as `UnsignedOriginatorEnvelope.originator_sequence_id` on the wire.

The identity is direct: `pkg/api/message/publish_worker.go`, `persistBatch`:

```go
batchInput.Add(types.GatewayEnvelopeRow{
    OriginatorNodeID:     originatorID,
    OriginatorSequenceID: prep.staged.ID,     // <-- staged BIGSERIAL becomes the sequence id
    ...
})
```

`gateway_time` is a separate `TIMESTAMP` column, set to `prep.staged.OriginatorTime` (the staging
time). It is **not** a read-order key on any normal query path — no query sorts or pages by it except
`GetNewestEnvelope` — but it is not unused either. It has three further uses:

| Use | Where |
| --- | --- |
| Bucketing payer usage into minutes: `floor(extract(epoch from gateway_time) / 60)::int AS minutes_since_epoch`, written to `unsettled_usage` | `pkg/db/migrations/00023_rename-envelope-blobs.up.sql`, the `u_prep` CTE of `insert_gateway_envelope_batch_v3` |
| Bucketing congestion counts into the same minute grid, written to `originator_congestion` | same migration, the `c_prep` CTE |
| Stored as a column of the per-originator high-water mark table (`MAX(gateway_time)` per statement) | `pkg/db/migrations/00018_add_latest_envelopes.up.sql`, `update_latest_envelope_v2` and `gateway_envelopes_latest` |

So `gateway_time` is load-bearing for **accounting** and is carried on the latest-envelope row, even
though ordinary reads never order by it. **[DROP]** the accounting uses with the payer machinery; the
column itself is worth keeping as a plain "when did this node store it" timestamp.

### 7.2 How per-originator ordering is actually achieved

Three mechanisms stack:

**Mechanism 1 — serialized staging.** `insert_staged_originator_envelope_batch_v2`
(`pkg/db/migrations/00021_insert_staged_envelopes_batch-v2.up.sql`) takes

```sql
pg_advisory_xact_lock(hashtext('staged_originator_envelopes_sequence'))
```

as its first statement, then inserts `ORDER BY input.i`. Because the lock is transaction-scoped and held
across the inserts, **only one staging transaction can be assigning ids at a time**, which is what makes
`BIGSERIAL` order equal commit order. Without it, two concurrent transactions could take ids 5 and 6 and
commit in the order 6, 5 — producing a reader-visible gap that later fills in, breaking every cursor.

**Mechanism 2 — a single sequential publisher per node.** `publishWorker.processBatch`
(`pkg/api/message/publish_worker.go`) does, inside one transaction:

```text
SharedLockPartitionCreation           -- reader side of the partition lock
SelectAndLockStagedEnvelopes(100)     -- SELECT ... ORDER BY id ASC LIMIT 100 FOR UPDATE
prepareEnvelopes(...)                 -- sign each, compute fees and expiry
BulkFindOrCreatePayers(...)
InsertGatewayEnvelopeBatchV3(...)     -- one statement, all 100 rows
BulkDeleteStagedOriginatorEnvelopes(...)
```

`FOR UPDATE` plus `ORDER BY id ASC` means two publish workers cannot process overlapping ranges, and
each batch is inserted in one statement inside one transaction. So rows become visible in id order.

**Mechanism 3 — the invariant is asserted, not merely assumed.** Two places hard-crash on violation:

- `pkg/api/message/service.go`, `QueryEnvelopes`: `logger.Fatal("system invariant broken: unsorted
  envelope stream", ...)`.
- `pkg/api/message/envelope_list.go`, the per-originator subscription query closure: the same check.

`zap.Logger.Fatal` exits the process. The authors chose to crash rather than serve out-of-order data.

The invariant is stated verbatim in `pkg/api/message/subscribe.go`:

> "the system invariant that each originator's envelopes become visible in sequence order (one
> sequential writer per originator); a row committing out of order after a reader passed it would be
> undeliverable stream-wide"

**This is the single most important thing to preserve.** Everything downstream — cursor dedupe, the
wave ceiling pin, the live high-water mark, XIP-83's exactly-once guarantee — rests on it.

### 7.3 The `FillMissingOriginators` subtlety

`pkg/db/types.go`, `FillMissingOriginators(vc VectorClock, allOriginators []uint32)` adds a `0` entry for
every known originator the client's cursor does not name.

It matters because `SelectGatewayEnvelopesByTopics` **cross-joins topics with cursor entries**. An
originator absent from the cursor map produces no `(topic, originator)` pair at all, so its envelopes
are never even probed for — silently, not as "treated as sequence 0."

The "known originators" list comes from `db.NewCachedOriginatorList`
(`pkg/db/originator_list.go`), a **TTL cache** over `SelectOriginatorNodeIDs` with TTL
`--api.originator-cache-ttl`, default **5 minutes**.

Consequence, documented in `pkg/api/message/subscribe.go`: an originator that neither the cache nor the
client's cursor names is invisible to a catch-up scan. For live subscriptions this is covered because
the listener registers by topic (independent of the originator list). For `history_only` bounded sync
it is **not** covered, and the code accepts that as "an accepted eventual-consistency property."

**[DROP]** — with one sequence there is no originator set to fill, and this entire class of bug
disappears.

### 7.4 Cursor semantics, precisely

| Aspect | Behavior | Source |
| --- | --- | --- |
| Type | `map<uint32 originator_node_id, uint64 sequence_id>` | `envelopes.pb.go`, `Cursor` |
| Delivery predicate | `env.seq > cursor[env.originator]` | `advanceLive`, `advanceTopicCursors`, and the SQL `> ce.seq_id` |
| Missing originator | treated as `0` **only after** `FillMissingOriginators` | §7.3 |
| "From the beginning" | empty map (or nil `last_seen`) | `buildTopicCursors` |
| Advancement | in-place, in Go, as envelopes are sent | `advanceTopicCursors` / `advanceLive` |
| Max entries | 100 (`maxVectorClockLength`) on query/subscribe filters; 1 000 000 total per XIP-83 Mutate | `validateQuery`, `maxMutateCursorEntries` |
| Returned to client? | **never** — no response carries a cursor | all response protos |
| Range validation | only in XIP-83 `handleMutate` (`nodeID <= MaxInt32`, `seqID <= MaxInt64`) | `pkg/api/message/subscribe.go` |

The last two rows are notable. Because no response echoes a cursor, **every client must reconstruct its
own resume position** from the envelopes it received. And because only the XIP-83 path range-checks
cursor values, an out-of-range value on the older paths is silently dropped by the query.

### 7.5 Sort order, by endpoint

| Endpoint | Order |
| --- | --- |
| `QueryEnvelopes` (topics) | `(originator_node_id, originator_sequence_id)` ascending |
| `QueryEnvelopes` (single originator) | `originator_sequence_id` ascending |
| `QueryEnvelopes` (multi originator) | `(originator_node_id, originator_sequence_id)` ascending |
| `GetNewestEnvelope` | `(topic, gateway_time DESC)`, one row per topic — **see the TODO in §6.8** |
| `SubscribeTopics` catch-up | per (topic, originator) bursts within a 500-topic chunk |
| `SubscribeTopics` live | worker dispatch order — per originator ascending, no cross-topic merge |
| `Subscribe` wave replay | `(originator_node_id, originator_sequence_id)` ascending across **all** the wave's topics — one merged scan |
| `Subscribe` live tail | worker dispatch order, per originator ascending, across all live topics |

There is **no global time ordering** anywhere. Two envelopes from different originators have no defined
relative order at all.

### 7.6 Idempotency and deduplication

| Layer | Mechanism | Effective? |
| --- | --- | --- |
| Staged insert | `ON CONFLICT DO NOTHING` on a fresh `BIGSERIAL` PK | **No** — a retried publish creates duplicates |
| Gateway meta insert | `ON CONFLICT DO NOTHING` on `(originator_node_id, originator_sequence_id)` | Yes — but only against a replay of the *same* sequence id |
| Gateway blob insert | same | Yes |
| `address_log` upsert | monotonic guard: only updates when the new `association_sequence_id` is strictly higher | Yes |
| Identity update indexer | `latestSequenceID >= msgSent.SequenceId` → skip | Yes |
| `payer_reports` / attestations | `ON CONFLICT (id)` / `(payer_report_id, node_id) DO NOTHING` | Yes |
| Read path | cursor comparison in `advanceLive` / `advanceTopicCursors` | Yes, per stream |

**The gap**: there is **no content-addressed deduplication of publishes**. A client that retries
`PublishClientEnvelopes` after a timeout gets a second copy of its message with a new sequence id. The
blockchain path is naturally idempotent (a re-submitted transaction either reverts or produces one
event), but the node path is not. A new backend should add a client-supplied idempotency key or a
content hash unique index.

---

## 8. Database schema

23 migrations, `pkg/db/migrations/00001_*.up.sql` through `00023_*.up.sql`. The policy is
**append-only**: superseded functions are left in place rather than dropped, so several dead
`_v1`/`_v2` plpgsql functions still exist.

### 8.1 Table inventory

| Table | Purpose | Fate |
| --- | --- | --- |
| `gateway_envelopes_meta` | envelope metadata, LIST+RANGE partitioned | **KEEP** (unpartitioned or time-partitioned) |
| `gateway_envelopes_blob` | envelope bytes, same partitioning | **KEEP** (consider merging) |
| `gateway_envelopes_view` | the join of the two | KEEP |
| `gateway_envelopes_latest` | per-originator high-water mark, trigger-maintained | **KEEP** (one row) |
| `staged_originator_envelopes` | the ordering queue | **KEEP or eliminate** — see §19 |
| `address_log` | address → inbox id projection | **KEEP** |
| `node_info` | this node's identity, singleton | DROP |
| `payers` | payer address → id | DROP |
| `unsettled_usage` | per-(payer, originator, minute) metering | DROP |
| `payer_ledger_events` | balance event log | DROP |
| `originator_congestion` | per-minute message counts | DROP |
| `payer_reports`, `payer_report_attestations` | billing settlement | DROP |
| `nonce_table` | Ethereum transaction nonce allocation | DROP |
| `latest_block`, `blockchain_messages` | chain indexer state | DROP |
| `migration_tracker`, `migration_dead_letter_box` | v2→v3 backfill bookkeeping | DROP |

### 8.2 `gateway_envelopes_meta`

```sql
CREATE TABLE IF NOT EXISTS gateway_envelopes_meta
(
    gateway_time           timestamp NOT NULL DEFAULT now(),
    originator_node_id     int       NOT NULL,
    originator_sequence_id bigint    NOT NULL,
    topic                  bytea     NOT NULL,
    payer_id               int REFERENCES payers(id),
    expiry                 bigint NOT NULL,
    PRIMARY KEY (originator_node_id, originator_sequence_id)
) PARTITION BY LIST (originator_node_id);
```

`payer_id` is nullable — blockchain-sourced envelopes have no payer. `expiry` is a Unix-seconds
`bigint`, `math.MaxInt64` for chain-sourced rows.

Live indexes (after the `00019` add/drop pass):

```sql
-- 00005, kept
CREATE INDEX gem_expiry_idx
    ON gateway_envelopes_meta (expiry)
    INCLUDE (originator_node_id, originator_sequence_id)
    WHERE expiry IS NOT NULL;

CREATE INDEX IF NOT EXISTS gem_topic_time_desc_idx
    ON gateway_envelopes_meta (topic, gateway_time DESC)
    INCLUDE (originator_node_id, originator_sequence_id);

-- 00019, the covering index the V3b LATERAL queries were designed around
CREATE INDEX IF NOT EXISTS gem_topic_orig_seq_idx
    ON gateway_envelopes_meta (topic, originator_node_id, originator_sequence_id)
    INCLUDE (gateway_time);
```

Dropped by `00019`: `gem_topic_time_idx`, `gem_time_node_seq_idx`, `gem_originator_node_id` — the last
became redundant once `originator_node_id` was the partition key itself.

Index usage by query:

| Query | Index |
| --- | --- |
| `SelectGatewayEnvelopesByTopics`, `...ByPerTopicCursors`, `SelectGatewayEnvelopesWaveScan` | `gem_topic_orig_seq_idx` (covering — `gateway_time` is INCLUDEd so the heap is not touched for the meta half) |
| `SelectGatewayEnvelopesBySingleOriginator`, `...ByOriginators` | the primary key, within one partition |
| `SelectNewestFromTopics` | `gem_topic_time_desc_idx` |
| Pruning row deletes | `gem_expiry_idx` |
| `SelectOriginatorCeilings` (`MAX(seq)` per originator) | the primary key — a per-partition index-only backward scan |

Because both partition levels are native Postgres partitioning, an index created on the parent
propagates to every leaf automatically.

### 8.3 `gateway_envelopes_blob`

```sql
CREATE TABLE IF NOT EXISTS gateway_envelopes_blob
(
    originator_node_id     int    NOT NULL,
    originator_sequence_id bigint NOT NULL,
    originator_envelope    bytea  NOT NULL,
    PRIMARY KEY (originator_node_id, originator_sequence_id),
    FOREIGN KEY (originator_node_id, originator_sequence_id)
        REFERENCES gateway_envelopes_meta(originator_node_id, originator_sequence_id) ON DELETE CASCADE
) PARTITION BY LIST (originator_node_id);
```

Named `gateway_envelope_blobs` until `00023_rename-envelope-blobs.up.sql` renamed the table and every
child partition, deepest-first.

**The meta/blob split is a deliberate design choice**: the hot index scans touch only the narrow meta
table, and the wide `bytea` payload is fetched in a second LATERAL join only for the rows that survived
filtering. The `ON DELETE CASCADE` means pruning meta rows automatically removes blobs.

### 8.4 `gateway_envelopes_latest` and its trigger

```sql
CREATE TABLE gateway_envelopes_latest (
    originator_node_id     int PRIMARY KEY,
    originator_sequence_id bigint NOT NULL,
    gateway_time           timestamp NOT NULL
);
```

Maintained **entirely by a Postgres trigger**, never by application code. Current version, from
`00018_add_latest_envelopes.up.sql`:

```sql
CREATE OR REPLACE FUNCTION update_latest_envelope_v2()
RETURNS trigger AS $$
BEGIN
    INSERT INTO gateway_envelopes_latest as g
    SELECT originator_node_id, MAX(originator_sequence_id), MAX(gateway_time)
    FROM new
    GROUP BY originator_node_id
    ON CONFLICT (originator_node_id)
    DO UPDATE
        SET originator_sequence_id = EXCLUDED.originator_sequence_id,
            gateway_time = EXCLUDED.gateway_time
        WHERE EXCLUDED.originator_sequence_id > g.originator_sequence_id;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER gateway_latest_upd_v2
    AFTER INSERT ON gateway_envelopes_meta
    REFERENCING NEW TABLE AS new
    FOR EACH STATEMENT EXECUTE FUNCTION update_latest_envelope_v2();

DROP TRIGGER IF EXISTS gateway_latest_upd ON gateway_envelopes_meta;
```

Three things to note:

1. **Statement-level**, not row-level (`REFERENCING NEW TABLE AS new`, `FOR EACH STATEMENT`) — it fires
   once for a 100-row batch insert, aggregating first. `00006`'s row-level predecessor was replaced for
   exactly this reason.
2. The `WHERE EXCLUDED.originator_sequence_id > g.originator_sequence_id` guard makes it monotonic even
   under concurrent or out-of-order commits.
3. Because it is a real trigger, it fires for **every** insert path — single, batch, stored function,
   raw SQL — with no application cooperation.

This table backs `SelectVectorClock`, `GetLatestSequenceId`, and `SelectOriginatorNodeIDs`, and thus the
metadata API, the publish worker's `lastProcessed`, the subscribe worker's startup cursor, and the
originator list cache.

**[KEEP]** — a trigger-maintained high-water mark is exactly right, and with one sequence it becomes a
single-row table (or a Postgres sequence's `last_value`).

### 8.5 `staged_originator_envelopes`

```sql
CREATE TABLE staged_originator_envelopes(
 id BIGSERIAL PRIMARY KEY,
 originator_time TIMESTAMP NOT NULL DEFAULT now(),
 topic BYTEA NOT NULL,
 payer_envelope BYTEA NOT NULL
);
```

No indexes beyond the PK. Rows are deleted by the publish worker after a successful gateway insert, so
the table is a short queue, not a log.

### 8.6 `address_log`

```sql
CREATE TABLE address_log(
 address TEXT NOT NULL,
 inbox_id BYTEA NOT NULL,
 association_sequence_id BIGINT,
 revocation_sequence_id BIGINT,
 PRIMARY KEY (address, inbox_id)
);
```

Both sequence columns are nullable. `inbox_id` is stored raw but crosses the SQL boundary as hex
(`decode(@inbox_id,'hex')` on write, `encode(a.inbox_id,'hex')` on read).

### 8.7 Remaining tables (condensed)

```sql
CREATE TABLE node_info(                          -- singleton: PK + CHECK(singleton_id=1)
 node_id INTEGER NOT NULL, public_key BYTEA NOT NULL,
 singleton_id SMALLINT PRIMARY KEY DEFAULT 1,
 CONSTRAINT is_singleton CHECK (singleton_id = 1));

CREATE TABLE payers(id SERIAL PRIMARY KEY, address TEXT NOT NULL UNIQUE);

CREATE TABLE unsettled_usage(
    payer_id INTEGER NOT NULL, originator_id INTEGER NOT NULL,
    minutes_since_epoch INTEGER NOT NULL, spend_picodollars BIGINT NOT NULL,
    last_sequence_id BIGINT NOT NULL, message_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (payer_id, originator_id, minutes_since_epoch));
CREATE INDEX idx_unsettled_usage_originator_id_minutes_since_epoch
    ON unsettled_usage(originator_id, minutes_since_epoch DESC);
-- 00017 adds FOREIGN KEY (payer_id) REFERENCES payers(id)

CREATE TABLE payer_ledger_events(
    event_id BYTEA PRIMARY KEY, payer_id INTEGER NOT NULL,
    amount_picodollars BIGINT NOT NULL,
    event_type SMALLINT NOT NULL,   -- 0 deposit,1 withdrawal,2 settlement,3 canceled withdrawal,4 reorg reversal
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX idx_payer_ledger_events_payer_id ON payer_ledger_events(payer_id);
-- 00017 adds FOREIGN KEY (payer_id) REFERENCES payers(id)

CREATE TABLE originator_congestion(
    originator_id INTEGER NOT NULL, num_messages INTEGER NOT NULL DEFAULT 0,
    minutes_since_epoch INTEGER NOT NULL,
    PRIMARY KEY (originator_id, minutes_since_epoch));

CREATE TABLE payer_reports (
    id BYTEA PRIMARY KEY, originator_node_id INT NOT NULL,
    start_sequence_id BIGINT NOT NULL, end_sequence_id BIGINT NOT NULL,
    end_minute_since_epoch INT NOT NULL, payers_merkle_root BYTEA NOT NULL,
    active_node_ids INT[] NOT NULL,
    submission_status SMALLINT NOT NULL DEFAULT 0,   -- 0 pending,1 submitted,2 settled,3 rejected
    attestation_status SMALLINT NOT NULL DEFAULT 0,  -- 0 pending,1 approved,2 rejected
    created_at TIMESTAMPTZ DEFAULT NOW(), submitted_report_index INTEGER NULL);
CREATE INDEX payer_reports_submission_status_created_idx  ON payer_reports (submission_status, created_at);
CREATE INDEX payer_reports_attestation_status_created_idx ON payer_reports (attestation_status, created_at);

CREATE TABLE payer_report_attestations (
    payer_report_id BYTEA NOT NULL,   -- deliberately NOT a FK: may arrive before the report
    node_id BIGINT NOT NULL, signature BYTEA NOT NULL, created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (payer_report_id, node_id));
CREATE INDEX payer_report_attestations_payer_report_id_idx ON payer_report_attestations (payer_report_id);

CREATE TABLE nonce_table (nonce BIGINT PRIMARY KEY, created_at TIMESTAMP DEFAULT NOW());

CREATE TABLE latest_block(
 contract_address TEXT NOT NULL PRIMARY KEY, block_number BIGINT NOT NULL, block_hash BYTEA);

CREATE TABLE blockchain_messages(
 block_number BIGINT NOT NULL, block_hash BYTEA NOT NULL,
 originator_node_id INT NOT NULL, originator_sequence_id BIGINT NOT NULL,
 is_canonical BOOLEAN NOT NULL DEFAULT TRUE,
 PRIMARY KEY (block_number, block_hash, originator_node_id, originator_sequence_id),
 FOREIGN KEY (originator_node_id, originator_sequence_id)
     REFERENCES gateway_envelopes_meta(originator_node_id, originator_sequence_id));
CREATE INDEX idx_blockchain_messages_block_canonical ON blockchain_messages(block_number, is_canonical);
```

### 8.7a The v2→v3 migration tables

These two are named in the inventory (§8.1) but were previously left undefined. They belong to the
v2→v3 backfill (`pkg/migrator/*`) and are **[DROP]** for a new backend — but they are part of the
current schema, and the dead-letter box carries a pattern worth noting.

**`migration_tracker`** — one row per source table, holding the backfill high-water mark
(`pkg/db/migrations/00011_add-migration-tracker.up.sql`):

```sql
CREATE TABLE migration_tracker(
 source_table TEXT NOT NULL PRIMARY KEY,
 last_migrated_id BIGINT NOT NULL DEFAULT 0,
 created_at TIMESTAMP NOT NULL DEFAULT NOW(),
 updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

INSERT INTO migration_tracker (source_table, last_migrated_id) VALUES
 ('group_messages', 0),
 ('inbox_log', 0),
 ('key_packages', 0),
 ('welcome_messages', 0);
```

`00013_add-commit-messages-migration.up.sql` seeds a **fifth** row:

```sql
INSERT INTO migration_tracker (source_table, last_migrated_id) VALUES
 ('commit_messages', 0);
```

So the table ships with five seed rows: `group_messages`, `inbox_log`, `key_packages`,
`welcome_messages`, `commit_messages`. No indexes beyond the primary key.

**`migration_dead_letter_box`** — rows the backfill could not convert
(`pkg/db/migrations/00014_add-dead-letter-box.up.sql`):

```sql
CREATE TABLE IF NOT EXISTS migration_dead_letter_box(
 source_table TEXT NOT NULL,
 sequence_id BIGINT NOT NULL,
 payload BYTEA NOT NULL,
 reason TEXT NOT NULL,
 retryable BOOLEAN NOT NULL DEFAULT FALSE,
 added_at TIMESTAMP NOT NULL DEFAULT NOW(),
 retried_at TIMESTAMP NOT NULL DEFAULT NOW(),
 PRIMARY KEY (source_table, sequence_id)
);

-- Index for reports: query all failures for a source_table, ordered by added_at.
CREATE INDEX IF NOT EXISTS migration_dead_letter_box_source_table_added_at_idx
    ON migration_dead_letter_box (source_table, added_at);

-- Index for retry worker: query retryable records ordered by retried_at (oldest first).
CREATE INDEX IF NOT EXISTS migration_dead_letter_box_retryable_retried_at_idx
    ON migration_dead_letter_box (retried_at)
    WHERE retryable = TRUE;
```

Two indexes with two distinct jobs: the first serves operator reporting per source table, the second
is a **partial** index serving the retry worker's oldest-first scan over only the retryable rows.

The same migration adds **two plpgsql functions**, both taking a content-derived advisory lock —
`pg_advisory_xact_lock(hashtext('migration_dead_letter_box_sequence'))` — the same pattern used for
staged-envelope sequencing (§7.2):

```sql
CREATE FUNCTION insert_migration_dead_letter_box(p_source_table TEXT, p_sequence_id BIGINT,
    p_payload BYTEA, p_reason TEXT, p_retryable BOOLEAN)
 RETURNS SETOF migration_dead_letter_box AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtext('migration_dead_letter_box_sequence'));
 RETURN QUERY INSERT INTO migration_dead_letter_box(source_table, sequence_id, payload, reason, retryable)
  VALUES(p_source_table, p_sequence_id, p_payload, p_reason, p_retryable)
 ON CONFLICT (source_table, sequence_id)
  DO UPDATE SET reason = EXCLUDED.reason, payload = EXCLUDED.payload,
                retryable = EXCLUDED.retryable, retried_at = NOW()
 RETURNING *;
END; $$ LANGUAGE plpgsql;

CREATE FUNCTION delete_migration_dead_letter_box(source_table TEXT, sequence_id BIGINT)
 RETURNS BOOLEAN AS $$
DECLARE deleted_count INTEGER;
BEGIN
 PERFORM pg_advisory_xact_lock(hashtext('migration_dead_letter_box_sequence'));
 DELETE FROM migration_dead_letter_box
 WHERE migration_dead_letter_box.source_table = delete_migration_dead_letter_box.source_table
  AND migration_dead_letter_box.sequence_id = delete_migration_dead_letter_box.sequence_id;
 GET DIAGNOSTICS deleted_count = ROW_COUNT;
 RETURN deleted_count > 0;
END; $$ LANGUAGE plpgsql;
```

The upsert is idempotent on `(source_table, sequence_id)` and refreshes `retried_at` on every retry,
which is what makes the partial index's oldest-first ordering meaningful. **[KEEP the pattern]** — a
dead-letter table with a retryable flag, a reason string, and a partial index for the retry worker is
a good shape for any backfill or ingest path, independent of this particular migration.

### 8.8 Notable query patterns worth stealing

Beyond the ones already shown, three patterns recur and are worth carrying forward:

**Optional-filter queries.** `FetchPayerReports` (`pkg/db/sqlc/payer_reports.sql`) uses
`sqlc.narg(...)` with the `(@x IS NULL OR col = @x)` idiom for a dozen optional predicates in one
prepared statement, plus a `COUNT(...) OVER (PARTITION BY pr.id)` window for a joined count.

**Compare-and-set status updates.** `SetReportAttestationStatus` / `SetReportSubmissionStatus` use
`WHERE id = @report_id AND status = ANY(@prev_status[])` — an optimistic state machine transition in one
statement.

**Gap-filling with `SKIP LOCKED`.** `nonce_table` + `fill_nonce_gap()` + `GetNextAvailableNonce`
(`... ORDER BY nonce ASC LIMIT 1 FOR UPDATE SKIP LOCKED`) allocate unique, gapless integers to
concurrent workers. `pkg/db/sequences_test.go`, `TestConcurrentReads` proves 20 concurrent transactions
each get a distinct value. The same shape would work for a work queue in the new backend.

### 8.9 Connection handling

`pkg/db/db.go` and `pkg/db/pgx.go`:

- Connections go through `pgxpool`, wrapped into `database/sql` via `stdlib.OpenDBFromPool`.
- `statement_timeout` is set as a pgx `RuntimeParam` (milliseconds) at pool-config time, from
  `--db.read-timeout` / `--db.write-timeout` (both default 10 s).
- `db.Handler` holds a **writer** pool and an optional **read-replica** pool, exposing
  `Write()` / `Read()` / `WriteQuery()` / `ReadQuery()`. `ReadQuery()` falls back to the writer when no
  replica is configured. Every read-heavy query in this document goes through `ReadQuery()`.
- Query tracing is tagged with `role` ("reader"/"writer") specifically to make replica lag debuggable.
- `SetLocalWorkMem` (`pkg/db/sqlc/configuration.sql`) does `set_config('work_mem', $1, true)` — a
  transaction-local override for the heavy LATERAL queries.

---

## 9. Partitioning and the advisory lock

### 9.1 Two-level partitioning

Both `gateway_envelopes_meta` and `gateway_envelopes_blob` are partitioned twice:

- **Level 1, LIST by `originator_node_id`** — `gateway_envelopes_meta_o0`, `_o1`, `_o100`, …
- **Level 2, RANGE by `originator_sequence_id`** in fixed bands of `GatewayEnvelopeBandWidth = 1_000_000`
  (`pkg/db/types.go`; mirrored as the plpgsql default `p_band_width bigint DEFAULT 1000000`) —
  `gateway_envelopes_meta_o0_s0_1000000`, `_s1000000_2000000`, …

Band start: `v_band_start := (p_originator_sequence_id / p_band_width) * p_band_width`.

### 9.2 The three generations of partition functions

| Generation | Migration | Approach |
| --- | --- | --- |
| v1 | `00002` | `CREATE TABLE ... PARTITION OF ... FOR VALUES ...` — one DDL statement |
| v2 | `00015` | `CREATE TABLE IF NOT EXISTS x (LIKE parent INCLUDING DEFAULTS INCLUDING CONSTRAINTS, CONSTRAINT ... CHECK (...))` as a **standalone** table, then `ALTER TABLE parent ATTACH PARTITION x FOR VALUES ...`, then drop the temporary CHECK |
| v3 | `00023` | Same as v2 but for the renamed blob table; reuses the unchanged v2 meta makers |

The v2 rewrite matters: the temporary CHECK constraint lets Postgres skip the full-table validation scan
during ATTACH. Each v2/v3 function wraps its ATTACH in an `EXCEPTION WHEN OTHERS` block that swallows
**only** the specific race, matching on the message text `'is already a partition'` — the migration's own
comment explains that SQLSTATE 42809 is not exclusive to that case, so matching on the code alone would
be wrong.

Production entry point:

```sql
CREATE FUNCTION ensure_gateway_parts_v3(
    p_originator_node_id int, p_originator_sequence_id bigint, p_band_width bigint DEFAULT 1000000
) RETURNS void LANGUAGE plpgsql AS $$
DECLARE v_band_start bigint := (p_originator_sequence_id / p_band_width) * p_band_width;
BEGIN
    PERFORM make_meta_originator_part_v2(p_originator_node_id);
    PERFORM make_blob_originator_part_v3(p_originator_node_id);
    PERFORM make_meta_seq_subpart_v2(p_originator_node_id, v_band_start, v_band_start + p_band_width);
    PERFORM make_blob_seq_subpart_v3(p_originator_node_id, v_band_start, v_band_start + p_band_width);
END$$;
```

The v1/v2 blob functions and `insert_gateway_envelope_batch`/`_v2` remain in the schema per the
append-only policy and will now fail at runtime, since they reference the pre-rename table name.

### 9.3 The reader/writer advisory lock

`pkg/db/advisory_lock.go`:

```go
const (
 LockKindIdentityUpdateInsert LockKind = 0x00
 LockKindAttestationWorker    LockKind = 0x01
 LockKindSubmitterWorker      LockKind = 0x02
 LockKindSettlementWorker     LockKind = 0x03
 LockKindGeneratorWorker      LockKind = 0x04
 LockKindPartitionCreation    LockKind = 0x05
)
const partitionCreationLockKey = int64(LockKindPartitionCreation)
```

Backed by three sqlc queries (`pkg/db/sqlc/advisory_locks.sql`):

```sql
-- name: AdvisoryLockWithKey :exec
SELECT pg_advisory_xact_lock(@locking_key);
-- name: SharedAdvisoryLockWithKey :exec
SELECT pg_advisory_xact_lock_shared(@locking_key);
-- name: TryAdvisoryLockWithKey :one
SELECT pg_try_advisory_xact_lock(@locking_key) as lock_succeeded;
```

- Ordinary inserts take the **shared** lock (`AdvisoryLocker.SharedLockPartitionCreation`).
- Partition creation takes the **exclusive** lock (`AdvisoryLocker.LockPartitionCreation`).
- Both are `_xact` variants — released automatically at transaction end, never manually.
- The key is **global, not per-originator** — deliberately, to prevent cross-originator deadlocks.

The rationale is quoted verbatim in `pkg/db/advisory_lock.go`:

> "`ensure_gateway_parts_v3` ATTACHes partitions to the shared `gateway_envelopes_meta` and
> `gateway_envelopes_blob` parents. Because of the blob→meta and meta→payers foreign keys, each ATTACH
> validates the FK and takes `ShareRowExclusiveLock` on both parents once they hold data. That conflicts
> with the `RowExclusiveLock` ordinary inserts take, and concurrent transactions acquire the parents'
> locks in opposite orders and deadlock (SQLSTATE 40P01). Running partition creation as the exclusive
> writer, while inserts hold the shared lock, guarantees DDL never overlaps DML, removing the conflict."

The exclusive lock **must run in its own transaction**, never as an in-place upgrade from a shared lock
held by an insert — two upgraders would deadlock against each other. That is why
`db.EnsureGatewayPartitions` (`pkg/db/gateway_envelope.go`) opens a fresh `RunInTx` solely to take the
exclusive lock and call `EnsureGatewayPartsV3`.

This lock is the subject of the commit immediately before the XIP-83 work:
`4b14f2a5 fix(db): eliminate gateway-envelope partition-creation deadlock via reader/writer advisory lock`.

### 9.4 The savepoint-and-retry insert pattern

`pkg/db/gateway_envelope.go`, `InsertGatewayEnvelopeWithChecksTransactional`, running **inside** the
caller's transaction:

1. `SharedLockPartitionCreation`.
2. `SAVEPOINT sp_part` (`InsertSavePoint`).
3. `InsertGatewayEnvelopeV3`.
4. Success → `RELEASE SAVEPOINT sp_part`.
5. Error whose message contains `"no partition of relation"` (`isNoPartitionErr`) →
   `ROLLBACK TO SAVEPOINT sp_part`, return the sentinel `ErrGatewayPartitionMissing`.
6. Any other error → returned as-is, leaving the transaction aborted.

The caller (`InsertGatewayEnvelopeAndIncrementUnsettledUsage`, or `publishWorker.processBatch` for the
batch form) catches `ErrGatewayPartitionMissing`, calls `EnsureGatewayPartitions` in a **separate**
transaction under the exclusive lock, and retries the whole transaction **once**.

The savepoint is what makes this possible: without it, the failed insert would poison the enclosing
transaction and the retry would have to redo everything, including re-validating the identity update.

There is also a **proactive** partition worker (`pkg/db/worker/worker.go`) that checks every
`DefaultCheckInterval` = 30 min whether any originator has filled `DefaultFillThreshold` = **70%** of
its current band, and pre-creates the next one — so the reactive path is a fallback, not the norm.

### 9.5 Would the new backend need any of this?

Almost none of it. The partitioning exists because of two decisions the new design removes:

1. **LIST by originator** exists only because there are multiple originators.
2. **RANGE by sequence in 1 M bands** exists so that whole partitions can be dropped when a payer report
   settles a range — pruning by DDL rather than by DELETE.

With a single sequence, if partitioning is still wanted for pruning, partition by **time** (e.g. monthly
`RANGE (created_at)`), which matches how retention actually works and drops whole months cheaply. Then:

- no LIST level,
- no per-originator band arithmetic,
- no advisory lock (a single-level time partition can be pre-created far ahead by a cron),
- no savepoint retry dance.

---

## 10. Identity updates end to end

This is where xmtpd's global ordering actually comes from, and it is the section most likely to surprise.

### 10.1 The path

Identity updates **never** flow through `PublishPayerEnvelopes`. That endpoint explicitly rejects them
(`preprocessPayerEnvelopes`: `"identity updates must be published via the blockchain"`). The full path:

```text
client
  └─► PayerApi.PublishClientEnvelopes             pkg/api/payer/service.go
        └─► shouldSendToBlockchain() == true       (always, for TopicKindIdentityUpdatesV1)
        └─► publishToBlockchain()
              └─► blockchainPublisher.PublishIdentityUpdate(ctx, inboxID, payload)
                    └─► IdentityUpdateBroadcaster contract on the app chain
                          └─► emits IdentityUpdateCreated{InboxId, SequenceId, Update}
                                                   ▲
                                                   │  the contract assigns SequenceId
  ┌────────────────────────────────────────────────┘
  │
  └─► indexer watches the contract log
        └─► IdentityUpdateStorer.StoreLog()        pkg/indexer/app_chain/contracts/identity_update_storer.go
              ├─ EnsureGatewayPartitions (out-of-band, exclusive lock)
              └─ transaction @ READ COMMITTED:
                   ├─ LockIdentityUpdateInsert (advisory, keyed on originator 1)
                   ├─ GetLatestSequenceId(originator 1)
                   ├─ if latest >= event.SequenceId -> skip (idempotent)
                   ├─ validateIdentityUpdate()  -> MLS validation service
                   ├─ InsertAddressLogsBatch / RevokeAddressFromLogBatch
                   └─ InsertGatewayEnvelopeWithChecksTransactional(originator=1, seq=event.SequenceId,
                                                                  expiry=math.MaxInt64)
```

### 10.2 How global ordering is achieved — precisely

**The smart contract is the sequencer.** `IdentityUpdateBroadcaster` assigns a monotonically increasing
`SequenceId` to every identity update, and the blockchain's own consensus makes that assignment a single
global total order that every node observes identically. Every node's indexer reads the same event log
and writes the same `(originator_node_id = 1, originator_sequence_id = <contract seq>)` rows.

So: **xmtpd does not order identity updates. The chain does.** Nodes are replicas of a chain-produced
log. The same applies to group-message commits and proposals, ordered by
`GroupMessageBroadcaster` under synthetic originator `0`.

This is the design's load-bearing assumption, and it is the one the new backend must replace outright.
With a single Postgres, ordering identity updates is a *local* problem — one sequence, one writer, done
— which is strictly simpler and strictly stronger (no reorg handling, no chain latency).

The three ordering mechanisms in the indexer, for completeness:

1. **Advisory lock**: `LockIdentityUpdateInsert(ctx, querier, constants.IdentityUpdateOriginatorID)`,
   keyed as `(nodeID << 8) | LockKindIdentityUpdateInsert`. Ensures only one HA worker processes an
   event at a time.
2. **READ COMMITTED, deliberately**. The comment in `identity_update_storer.go` is worth quoting:

   > "READ COMMITTED means each statement sees the latest committed state, so GetLatestSequenceId
   > reflects any inserts that committed after we waited on the advisory lock. At REPEATABLE READ we
   > would be stuck with the snapshot taken before the lock, which could cause duplicate processing or
   > serialization errors."

3. **Monotonic skip**: `if uint64(latestSequenceID) >= msgSent.SequenceId { skip }` — replaying the same
   log entry is a no-op.

### 10.3 Validation against prior updates

`IdentityUpdateStorer.validateIdentityUpdate`:

1. Read the inbox's existing identity-update envelopes:

   ```go
   querier.SelectGatewayEnvelopesByTopics(ctx, queries.SelectGatewayEnvelopesByTopicsParams{
       Topics:            []db.Topic{topic.NewTopic(topic.TopicKindIdentityUpdatesV1, inboxID[:]).Bytes()},
       RowLimit:          256,
       CursorNodeIds:     []int32{constants.IdentityUpdateOriginatorID},
       CursorSequenceIds: []int64{0},
   })
   ```

   Note **`RowLimit: 256`** — an inbox with more than 256 identity updates would silently validate
   against a truncated history. **[UNVERIFIED]** whether that is considered a real bound. The comment
   explains why `FillMissingOriginators` is not needed here: "Identity updates are exclusively produced
   by IdentityUpdateOriginatorID."

2. `mlsvalidate.GetAssociationStateFromEnvelopes(ctx, gatewayEnvelopes, newIdentityUpdate)` — which
   unwraps each stored envelope back down to its `IdentityUpdate` payload and calls the MLS validation
   service's `GetAssociationState(oldUpdates, newUpdates)`.

3. The service returns `AssociationStateResult{AssociationState, StateDiff}`.

### 10.4 Applying the diff to `address_log`

From `StateDiff`:

- `GetNewMembers()` filtered to `MemberIdentifier_EthereumAddress` → `InsertAddressLogsBatch`.
- `GetRemovedMembers()` likewise → `RevokeAddressFromLogBatch`.

Both use `association_sequence_id = int64(msgSent.SequenceId)` — **the contract's sequence id** — which
is why `GetAddressLogs`'s `MAX(association_sequence_id)` is a globally meaningful ordering.

The upsert guards make replay safe:

```sql
INSERT INTO address_log(address, inbox_id, association_sequence_id, revocation_sequence_id)
 VALUES (@address, decode(@inbox_id, 'hex'), @association_sequence_id, NULL)
ON CONFLICT (address, inbox_id)
 DO UPDATE SET revocation_sequence_id = NULL, association_sequence_id = @association_sequence_id
 WHERE (address_log.revocation_sequence_id IS NULL
  OR address_log.revocation_sequence_id < @association_sequence_id)
  AND address_log.association_sequence_id < @association_sequence_id;
```

An older update can never overwrite a newer one, in either direction.

### 10.5 Retryable versus permanent validation failures

`shouldRetryValidationError` (`identity_update_storer.go`) matches the error **string** against a list
of known association-validation messages:

```text
"Error creating association", "Multiple create operations detected", "XID not yet created",
"Signature validation failed", "not allowed to add", "Missing existing member",
"Legacy key is only allowed to be associated using a legacy signature with nonce 0",
"The new member identifier does not match the signer", "Wrong inbox_id specified on association",
"Signature not allowed for role", "Replay detected", "Deserialization error",
"Missing identity update", "Wrong chain id.",
"Invalid account address: Must be 42 hex characters, starting with '0x'.",
"are not a public identifier", "Conversion error"
```

A match → **non-recoverable** (the update is semantically invalid; skip it). No match → **recoverable**
(assume transient; retry). The function carries its own critique:

> "this approach is fragile as it depends on us creating new error messages for new validation errors.
> This function should rely on gRPC error codes instead, but it's not possible at the moment. Read
> <https://github.com/xmtp/libxmtp/issues/3130>"

**[KEEP the lesson]** — the new backend's validation boundary should return structured, typed errors so
the caller can distinguish "invalid forever" from "try again" without string matching.

### 10.6 Reorg handling

`pkg/indexer/app_chain/contracts/identity_update_reorg_handler.go` and the `blockchain_messages` table
(`is_canonical BOOLEAN`) exist to detect and undo chain reorganizations. **[DROP]** entirely — a
Postgres commit does not reorg.

---

## 11. MLS validation service

xmtpd delegates all MLS cryptographic validation to a **separate gRPC service** (address from
`--mls-validation.grpc-address`), which is a Rust binary from libxmtp.

### 11.1 Interface

`pkg/mlsvalidate/interface.go`:

```go
type MLSValidationService interface {
 ValidateKeyPackages(ctx, keyPackages [][]byte) ([]KeyPackageValidationResult, error)
 ValidateGroupMessages(ctx, groupMessages []*mlsv1.GroupMessageInput) ([]GroupMessageValidationResult, error)
 GetAssociationState(ctx, oldUpdates, newUpdates []*associations.IdentityUpdate) (*AssociationStateResult, error)
 GetAssociationStateFromEnvelopes(ctx, oldUpdates []queries.SelectGatewayEnvelopesByTopicsRow,
                                  newIdentityUpdate *associations.IdentityUpdate) (*AssociationStateResult, error)
}

type KeyPackageValidationResult struct {
 IsOk bool; InstallationKey []byte; Credential *identity_proto.MlsCredential
 Expiration uint64; ErrorMessage string
}
type GroupMessageValidationResult struct { GroupID string }
type AssociationStateResult struct {
 AssociationState *associations.AssociationState
 StateDiff        *associations.AssociationStateDiff
}
```

Underlying gRPC surface (`pkg/proto/mls_validation/v1/service_grpc.pb.go`) has **four** RPCs:
`ValidateGroupMessages`, `GetAssociationState`, `ValidateInboxIdKeyPackages`,
`VerifySmartContractWalletSignatures`. The last has **no Go wrapper** and is never called from xmtpd.

### 11.2 What is validated, and what is not

| Payload | Validated by the MLS service? | Where |
| --- | --- | --- |
| **Key packages** | **Yes** — `ValidateInboxIdKeyPackages` | `Service.validateKeyPackage`, on every publish |
| **Identity updates** | **Yes** — `GetAssociationState` against prior updates | `IdentityUpdateStorer.validateIdentityUpdate`, in the indexer |
| **Group messages (application)** | **No** — only a local content-type check | `Service.validateGroupMessage` via `deserializer.ShouldSendToBlockchain` |
| **Group messages (commit/proposal)** | **No MLS validation found** | routed to the chain; the chain contract does not validate MLS semantics |
| **Welcome messages** | **No validation at all** beyond AAD/topic consistency | — |
| **Commit log entries** | **Not validated; not stored; no handler registered** | see the note below |

Two of these are worth restating plainly:

- `MLSValidationService.ValidateGroupMessages` **is implemented but has no caller** anywhere outside its
  own tests and mocks. Group messages are never MLS-validated on the publish path.
- **Welcome messages are entirely unvalidated.** Any well-formed `ClientEnvelope` with a
  `WelcomeMessage` payload and a matching welcome topic is accepted.

**On commit logs, precisely.** There *is* a commit-log RPC pair in the protos this repository
vendors: `BatchPublishCommitLog` and `BatchQueryCommitLog` on the v3 `MlsApi`
(`pkg/proto/mls/api/v1/apiv1connect/mls.connect.go`, procedures
`/xmtp.mls.api.v1.MlsApi/BatchPublishCommitLog` and `/xmtp.mls.api.v1.MlsApi/BatchQueryCommitLog`).
What xmtpd does **not** have is any implementation: no registered handler (§5), no commit-log table
in the schema (§8), no validation, and no storage. The generated client and server interfaces exist
and compile; nothing in this tree serves them. So a replacement backend inherits no commit-log
behavior from xmtpd, but should be aware that ecosystem clients are generated against these two
procedures.

### 11.3 Key package validation details

`MLSValidationServiceImpl.ValidateKeyPackages` (`pkg/mlsvalidate/service.go`) builds the request with
`IsInboxIdCredential: true` **always** — legacy/basic credentials are never requested. Per-item results:

- `IsOk == false` → `{IsOk: false, ErrorMessage: ...}`;
- `IsOk == true` → `{IsOk: true, InstallationKey: response.GetInstallationPublicKey(), Expiration: ...}`.

Note `Credential` is **always left nil** even on success, because the response type does not carry it
back despite the Go struct having the field.

Failure handling is **per item**: one bad key package does not fail the batch. Contrast
`ValidateGroupMessages`, where a single `!IsOk` returns an error for the whole batch —
inconsistent, though moot since it has no caller.

### 11.4 Error mapping

`pkg/mlsvalidate/service.go` performs **no** gRPC-code mapping — it returns raw errors upward. All code
assignment happens at the caller:

| Caller | Mapping |
| --- | --- |
| `Service.validateKeyPackage` | transport error → `Internal`; empty results → `Internal`; `!IsOk` → `InvalidArgument` |
| `IdentityUpdateStorer.validateIdentityUpdate` | string-matched → non-recoverable or recoverable (§10.5) |

### 11.5 For the new backend

**[KEEP the boundary].** Delegating MLS validation to a separate service is a good split — it keeps the
MLS implementation in one language (Rust, shared with clients) rather than reimplemented per backend.
A Rust backend can go further and call the libxmtp validation crate **in-process**, eliminating a
network hop, a deployment, and the string-matching error taxonomy in one move.

**[FIX the coverage].** Decide deliberately whether group messages and welcomes should be validated.
Today they are not, which means the store will accept and durably persist malformed MLS content.

---

## 12. Subscriptions: the live delivery machine

Every streaming endpoint shares one delivery mechanism. Understanding it explains all of them.

### 12.1 It is DB polling, not LISTEN/NOTIFY

Confirmed by repository-wide grep: **there is no `LISTEN`, `NOTIFY`, or `pg_notify` anywhere in
`pkg/`.** The `notifier` channel in `publishWorker` and `DBSubscription` is a plain in-process Go
channel; it only wakes a poller already running in the same process and does nothing across processes.

### 12.2 `DBSubscription`

`pkg/db/subscription.go`:

```go
type PollableDBQuery[ValueType any, CursorType any] func(
 ctx context.Context, lastSeen CursorType, numRows int32,
) (results []ValueType, nextCursor CursorType, err error)

type PollingOptions struct {
 Interval time.Duration
 Notifier <-chan bool
 NumRows  int32
}
```

`Start()` runs a notify-or-timer loop:

```go
s.poll("startup")
timer := time.NewTimer(s.options.Interval)
for {
    timer.Reset(s.options.Interval)
    select {
    case <-s.ctx.Done():          close(s.updates); return
    case <-s.options.Notifier:    s.poll("notification")
    case <-timer.C:               s.poll("timer_fallback")
    }
}
```

`poll(trigger)` drains **all** available pages before returning:

```go
for {
    results, lastID, err := s.query(ctx, s.lastSeen, s.options.NumRows)
    if err != nil { ...; return }          // lastSeen NOT advanced; retried next poll
    if len(results) == 0 { return }
    s.lastSeen = lastID
    s.updates <- results                    // blocks until the consumer reads
    if int32(len(results)) < s.options.NumRows { return }
}
```

Each poll opens an APM span tagged with the trigger, for a specific diagnostic purpose stated in the
comment: "If you see lots of `timer_fallback` with num_results > 0, the notification poll is missing
data (likely due to read-replica lag)."

### 12.3 One subscription per originator

`pkg/api/message/envelope_list.go`, `subscriptionHandler.newSubscription`, creates one `DBSubscription`
per originator node id, each running:

```go
SelectGatewayEnvelopesBySingleOriginator(ctx, params{
    OriginatorNodeID: int32(id), CursorSequenceID: lastSeen, RowLimit: numRows,
})
```

with `Interval = SubscribeWorkerPollTime` = **100 ms** and `NumRows = subscribeWorkerPollRows` =
**1000** (`pkg/api/message/subscribe_worker.go`). The per-originator streams are merged into one channel
by a `funnel` (`pkg/api/message/funnel.go`).

The size choice is documented: "based on measurements in testnet using PG, we can poll at most 1000
elements in a large DB; this gives us sufficient throughput if being run continually."

**Implication for latency**: a published message becomes visible to subscribers after up to 100 ms of
poll delay, on top of the publish worker's own delay (up to 1 s ticker, or immediate on `notifyStagedPublish`).

### 12.4 The subscribe worker and its dispatch

`pkg/api/message/subscribe_worker.go`, `subscribeWorker`.

At startup it reads `SelectVectorClock` into an initial vector clock, resolves the originator set
(canonical nodes from the registry plus the reserved ids
`constants.GroupMessageOriginatorID`, `constants.IdentityUpdateOriginatorID`, and three migrator ids),
and starts one `DBSubscription` per originator. `monitorNodeChanges` adds subscriptions when the
registry announces new canonical nodes.

The main loop unmarshals each batch and fans out three ways:

| Dispatch | Function | Keying |
| --- | --- | --- |
| Originator listeners | `dispatchToOriginators` | nested loop over originators × envelopes |
| Topic listeners | `dispatchToTopics` | **one envelope at a time**, `topicListeners.getListeners(env.TargetTopic().String())` |
| Global listeners | `dispatchToGlobals` | the whole batch |

`dispatchToTopics` sends single-envelope slices, with the comment "we expect the number of envelopes
per-topic to be small in each tick." A hot topic therefore produces one channel send per envelope.

### 12.5 Backpressure: drop the listener

`dispatchToListeners` is **non-blocking**:

```go
select {
case l.ch <- envs:
    // delivered
default:
    // channel full -> close the listener
    s.closeListener(l)
}
```

Listener channels are `subscriptionBufferSize = 1024` deep. When one fills, the worker **closes it**,
which every stream handler observes as `!open` on its receive channel:

| Handler | Reaction |
| --- | --- |
| `Subscribe` (XIP-83) | `connect.CodeAborted`, `subscription closed: consumer too slow` |
| `SubscribeTopics` | logs `channel closed by worker`, returns `nil` — **the client sees a clean close, not an error** |
| `SubscribeEnvelopes` / `SubscribeOriginators` | same, returns `nil` |

That difference matters: on the legacy paths a slow consumer gets a **silent, successful-looking
stream close** and must infer from its own cursor that it missed data. XIP-83's `Aborted` is the
correct behavior.

Listener reaping is careful about a specific race — `closeListener` sets `l.closed = true` under
`l.topicsMu` before closing the channel, "so a concurrent `mutableSubscription.addTopics` observes it
atomically with its own `addListener` call," preventing both a leak and a send-on-closed-channel panic.
Lock order is always `topicsMu → listenersMap.mu`.

### 12.6 Catch-up semantics, compared

| Endpoint | Catch-up | Page size | Ordering | Live boundary signal |
| --- | --- | --- | --- | --- |
| `SubscribeEnvelopes` | one shared cursor, `catchUpWithSendFn` | 1000, 100 ms between pages | `(originator, sequence)` | none — an empty message is the keepalive |
| `SubscribeOriginators` | mandatory cursor, same | 1000 | `(originator, sequence)` | none |
| `SubscribeAllEnvelopes` | **none** — live tail only | — | worker order | none |
| `SubscribeTopics` | per-topic cursors, chunked 500 topics | 500 rows, min 10 per pair | per-(topic, originator) bursts | `CATCHUP_COMPLETE` status, once |
| `Subscribe` (XIP-83) | per-topic cursors, per wave, ceiling-pinned | 500 | one merged `(originator, sequence)` scan across the wave's topics | `TopicsLive` + `CatchupComplete` per wave, plus a per-frame `mutate_id` |

Only `Subscribe` gates live delivery behind catch-up completion. On `SubscribeTopics` the live listener
is registered *before* catch-up and its envelopes are deduped by `advanceTopicCursors` — which works,
but means live envelopes can be delivered interleaved with, and ahead of, older history for the same
topic.

### 12.7 Keepalives, compared

| Endpoint | Keepalive frame | Interval | Bidirectional? |
| --- | --- | --- | --- |
| `SubscribeEnvelopes` / `SubscribeOriginators` / `SubscribeAllEnvelopes` | an **empty response message** | `SendKeepAliveInterval` (30 s), reset on real traffic | no |
| `SubscribeTopics` | `StatusUpdate{WAITING}` | same | no |
| `Subscribe` | `Ping{nonce}` requiring a `Pong` | same, send-idle based | **yes** |
| `SubscribeSyncCursor` | a repeat of the current cursor | fixed 30 s, **not** reset on traffic | no |

Every one of them exists for the same documented reason, stated in `Service.doSubscribe`: "GRPC
keep-alives are not sufficient in some load balanced environments. We need to send an actual payload:
<https://github.com/xmtp/xmtpd/issues/669>". XIP-83 §Motivation is the generalization of that bug report.

### 12.8 Disconnect behavior

| Trigger | Effect |
| --- | --- |
| Client cancels / disconnects | `ctx.Done()` → handler returns `nil`; the worker reaps the listener on its next dispatch attempt |
| Service shutdown (`s.ctx.Done()`) | legacy handlers return `nil`; `Subscribe` returns `Unavailable: service is shutting down` |
| Consumer too slow | listener closed (§12.5) |
| Send fails | `Internal` on legacy paths; the sender goroutine records it and the writer surfaces it on `Subscribe` |
| Pong deadline | only `Subscribe`: `DeadlineExceeded` |

`Subscribe` additionally cancels its derived `streamCtx` on every return path and waits, bounded by
`keepAlive`, for the sender goroutine to stop before letting connect finalize the stream — with an
explicit comment acknowledging the residual race when a sender is wedged inside a non-cancelable
`stream.Send`.

---

## 13. Rate limiting

`pkg/ratelimiter/*`. Entirely **Redis-backed**; disabled by default.

### 13.1 Algorithm

A **token bucket with continuous refill**, implemented as an atomic Redis Lua script
(`pkg/ratelimiter/script.lua`), executed by `RedisLimiter.Allow` (`pkg/ratelimiter/redis_limiter.go`).

- A `Limit{Capacity, RefillEvery}` bucket starts full and refills at `Capacity / RefillEvery`, with
  fractional token counts.
- **Multiple limits are checked atomically together** (e.g. per-minute and per-hour): if any lacks
  tokens, none are decremented.
- Redis keys: `<prefix>:<subject>:ts` for a shared timestamp, plus `<prefix>:<subject>:1`, `:2`, … per
  limit (`RedisLimiter.buildKeys`).

A **separate** script (`stream_script.lua`) implements a plain atomic counter for concurrent-stream
limiting — not a token bucket. `RedisStreamLimiter.Acquire` does `GET` then conditional `INCR` +
`PEXPIRE`; `Release` does a clamped `DECR` + `PEXPIRE`; `RefreshTTL` just extends. The TTL is the crash
self-heal: a process that dies without releasing has its slot freed when the key expires.

There is **no in-memory limiter**. When `--rate-limit.enable=false`, `ratelimiter.Build` returns
`(nil, nil)` and there is no limiting at all.

### 13.2 The three limiters

Built by `ratelimiter.Build` (`pkg/ratelimiter/builder.go`):

| Limiter | Limits | Redis prefix | Applied to |
| --- | --- | --- | --- |
| Query | `{T2PerMinuteCapacity, 1 min}` + `{T2PerHourCapacity, 1 h}` | `<prefix>rl:t2:q` | the **four unary** `QueryApi` methods only (see below) |
| Opens | `{T2SubscribeOpensPerMinute, 1 min}` | `<prefix>rl:t2:o` | **only** `QueryApi.SubscribeTopics` opens |
| Stream | max count `T1MaxConcurrentSubscribeAll`, TTL `StreamTTL` | `<prefix>rl:streams:` | `NotificationApi.SubscribeAllEnvelopes` |

Defaults: 60/min and 1200/h for queries, 10/min subscribe opens, 2 concurrent firehose streams.

**Exactly which procedures are limited.** `pkg/interceptors/server/rate_limit.go`,
`QueryApiMethodFromProcedure`, is a closed `switch` over **four** generated procedure constants:

```go
case message_apiconnect.QueryApiQueryEnvelopesProcedure:    return MethodQueryEnvelopes, true
case message_apiconnect.QueryApiSubscribeTopicsProcedure:   return MethodSubscribeTopics, true
case message_apiconnect.QueryApiGetInboxIdsProcedure:       return MethodGetInboxIds, true
case message_apiconnect.QueryApiGetNewestEnvelopeProcedure: return MethodGetNewestEnvelope, true
}
return "", false
```

`QueryApi.Subscribe` — the XIP-83 bidi stream — is **not in that list**. Any procedure the switch does
not name returns `("", false)`, and both `WrapUnary` and `WrapStreamingHandler` then call `next(...)`
immediately, applying no limit at all.

`WrapStreamingHandler` narrows it further. Even for a procedure the switch *does* recognize, it does:

```go
if method != MethodSubscribeTopics {
    return next(ctx, conn)
}
```

So the opens limiter is charged for **`SubscribeTopics` and nothing else**.

**Consequence for XIP-83 `Subscribe`**: it has **no open limit, no mutation limit, no ping limit, and
no lifetime limit** — not from the interceptor, and not from the handler (§6.3 records that the
handler has no mutation- or ping-rate limit either). A client can open unlimited concurrent
`Subscribe` streams and send unlimited Mutate and Ping frames on each, with rate limiting fully
enabled. `SubscribeTopics`, the older and less capable endpoint, is the one that pays admission.
Correcting an earlier phrasing: it is **not** true that "all `QueryApi` methods" go through the query
bucket — the newest and most expensive one does not.

**[FIX]** — this is the largest rate-limiting gap in the tree. Any replacement must charge the
mutable stream, which is strictly more expensive to serve than the immutable one.

### 13.3 The subject (rate-limit key)

**Client IP**, never payer id or wallet. `pkg/utils/clientip/clientip.go`, `Extract(peerAddr, xff,
trusted)`:

- Parses the immediate peer address.
- If the peer is inside a trusted-proxy CIDR (`--rate-limit.trusted-proxy-cidrs`), peels one hop off
  `X-Forwarded-For`, repeating while the next hop is also trusted.
- IPv4 → dotted quad; **IPv6 → normalized to its /64 prefix**, so a whole /64 shares one bucket.
- Unparseable input → the sentinel `clientip.InvalidClientIPKey` = `"invalid"`, rather than letting
  arbitrary strings into the Redis keyspace.

For subscribe opens the key is `<clientIP>:opens`.

### 13.4 Cost model

`pkg/ratelimiter/cost.go`:

```go
func CostQuery(numTopics int) uint64 { return ceil(sqrt(max(numTopics, 1))) }
```

**Sublinear** — 100 topics costs 10, 10 000 topics costs 100. A zero-topic query costs the baseline 1.

`pkg/interceptors/server/rate_limit.go`, `ComputeCost`:

| Method | Cost |
| --- | --- |
| `QueryEnvelopes` | `CostQuery(len(topics))` |
| `SubscribeTopics` | `CostQuery(len(filters))` |
| everything else | 1 |

### 13.5 Tiers

`pkg/ratelimiter/classifier.go`, `ClassifyTier(ctx)`:

- **Tier 0** — authenticated node-to-node traffic. **Bypasses all limits.** Detected by reading
  `constants.VerifiedNodeRequestCtxKey{}` from the context, set by the auth interceptor. The classifier
  never re-verifies a JWT itself.
- **Tier 2** — unauthenticated edge clients. Subject to limits.
- Tier 1 is declared but explicitly unused: "v1 only distinguishes Tier 0 … and Tier 2."

Note the config flag for the stream limiter is named `--rate-limit.t-1-max-concurrent-subscribe-all`
despite being applied to Tier 2 traffic — a naming inconsistency, not a behavior one.

### 13.6 Circuit breaker and fail-open

`pkg/ratelimiter/circuit_breaker.go`, a consecutive-failure breaker:

| State | Behavior |
| --- | --- |
| Closed | calls pass; `failureThreshold` consecutive failures → Open |
| Open | short-circuited until `cooldown` elapses → HalfOpen |
| HalfOpen | one probe allowed; success → Closed, failure → Open with the timer reset |

Failures recorded while already Open are ignored, so in-flight callers cannot extend the cooldown.
Defaults: threshold **5**, cooldown **10 s**, per-call Redis timeout **50 ms**. Each of the three
limiters gets its own breaker instance.

`BreakerLimiter` and `BreakerStreamLimiter` both **fail open**:

- breaker open → bypass Redis, allow;
- inner Redis error → record a failure, **still allow** ("Redis outages must not block traffic");
- a genuine denial (`Allowed: false`) is not a failure and does not affect the breaker.

### 13.7 Errors

| Path | Code | Message | Wire message |
| --- | --- | --- | --- |
| Query limit exceeded | `ResourceExhausted` | `rate limit exceeded` | `request has failed` |
| Subscribe-opens limit exceeded | `ResourceExhausted` | `subscribe rate limit exceeded` | `request has failed` |
| `SubscribeTopics` handler-level admission | `ResourceExhausted` | `subscribe admission rate limit exceeded` | `request has failed` |
| Concurrent stream limit | `ResourceExhausted` | `concurrent stream limit exceeded` | `request has failed` |
| Any limiter error | — | fails open; logged `rate limiter error, allowing request` / `stream limiter error, allowing stream` | *(no error returned)* |

**Every rate-limit message is `ResourceExhausted`, so none of them reach the client** (§6.0). A
client sees the code only. All four denials are wire-indistinguishable.

Internal library errors (`ErrCostMustBeGreaterThanZero`, `ErrUnexpectedScriptResponse`,
`ErrNoLimitsProvided`, `ErrInvalidFailedLimit`, `pkg/ratelimiter/errors.go`) never reach clients.

### 13.8 Known gap

`applySubscribeAdmission` charges only for **opening** a stream, not for holding it. The comment is
explicit: "It does not track the stream's lifetime — continual billing of long-held streams is
intentionally deferred to xmtp/xmtpd#1957." So a client can open a stream for 10 000 filters, pay 100
tokens once, and hold it forever.

---

## 14. Authentication and interceptors

### 14.1 The node JWT

`pkg/authn/*`. Used only for **node-to-node** authentication, in the `node-authorization` header
(`constants.NodeAuthorizationHeaderName`).

**Claims** (`pkg/authn/claims.go`, `XmtpdClaims`):

| Claim | Value |
| --- | --- |
| `sub` | the issuing node's id, as a decimal string |
| `aud` | `[<target node id>]` — pins the token to one recipient |
| `iat` | now |
| `exp` | now + `tokenDuration` = **1 hour** |
| `version` | the issuing server's semver (custom claim, optional) |
| `iss` | **not set** |

**Signature**: a custom JWT method `SigningMethodSecp256k1` registered as `"ES256K"`
(`pkg/authn/signing_method.go`) — Ethereum secp256k1 ECDSA, **not** standard P-256 ES256. `Sign` hashes
via `utils.HashJWTSignatureInput` (which prefixes `"jwt|"`) then `ethcrypto.Sign`. `Verify` requires a
65-byte signature and an `*ecdsa.PublicKey`, splits r and s, and calls `ecdsa.Verify`.

**Verification** (`pkg/authn/verifier.go`, `RegistryVerifier.Verify`):

1. `jwt.ParseWithClaims` with a key callback that (a) rejects any signing method that is not
   `*SigningMethodSecp256k1`, (b) extracts the subject node id, (c) looks that node's `SigningKey` up in
   the **on-chain node registry**.
2. `validateAudience` — this node's own id must appear in `aud`.
3. `validateExpiry` — issued no more than `MaxClockSkew` (**2 min**) in the future; `exp >= iat`;
   lifetime `exp - iat <= maxTokenDuration` (**2 h**); not currently expired.

   **A missing `exp` or `iat` is a nil dereference, not a rejection.** `validateExpiry`
   (`pkg/authn/verifier.go`) is:

   ```go
   exp, err := token.Claims.GetExpirationTime()
   if err != nil { return fmt.Errorf("could not get expiration time: %w", err) }
   issuedAt, err := token.Claims.GetIssuedAt()
   if err != nil { return fmt.Errorf("could not get issuance time: %w", err) }

   if time.Since(issuedAt.Time) < MaxClockSkew*-1 { ... }
   if exp.Before(issuedAt.Time) { ... }
   ```

   In `golang-jwt/jwt/v5`, `RegisteredClaims.GetExpirationTime` and `GetIssuedAt` return
   **`(nil, nil)`** when the claim is absent — a nil `*NumericDate` with a **nil error**. The `err`
   checks therefore do not fire, and the very next line dereferences the nil pointer
   (`issuedAt.Time`, or `exp.Before(...)`).

   So a **correctly signed** token from a node in the registry, but with no `exp` or no `iat` claim,
   panics inside the auth interceptor rather than returning `Unauthenticated`. xmtpd's own
   `TokenFactory` always sets both, so this is unreachable from a well-behaved peer — but the
   verifier runs on attacker-influenced input, and reaching it requires only a valid signing key.
   Whether the panic kills the process or is recovered per-request depends on the connect/HTTP2 stack
   above it; either way it is not the intended `Unauthenticated`.

   **[FIX]** — check `exp == nil` and `issuedAt == nil` explicitly and reject. The new backend should
   treat "claim absent" and "claim malformed" as the same rejection.
4. `validateClaims` → `ClaimValidator.ValidateVersionClaimIsCompatible`, checking the peer's version
   against `>=MinCompatibleVersion, <(serverMajor+1).0.0` where `MinCompatibleVersion = "1.1.0"`.
   Returns a `CloseFunc` that closes out a per-version connection gauge.

**[DROP]** — the whole scheme depends on the on-chain node registry for public keys.

### 14.2 Auth is optional by default

`pkg/interceptors/server/auth.go`, `ServerAuthInterceptor`: if the `node-authorization` header is
**absent**, the request proceeds **unauthenticated**. Only a header that is present and invalid is
rejected. The code comment: "Handlers must check `VerifiedNodeRequestCtxKey` if authentication is
required."

Enforcement is opt-in per surface:

| Surface | Requirement |
| --- | --- |
| `ReplicationApi` | `RequireNodeAuthInterceptor` **only if** `--api.require-replication-node-auth` (default **false**) |
| `QueryApi`, `PublishApi`, `NotificationApi`, `MetadataApi` | never required; a verified node merely bypasses rate limits |
| `PayerApi` / `GatewayApi` | a different model — `GatewayInterceptor` resolves an `Identity` from headers/peer and runs `AuthorizePublishFn` authorizers, but only on `PublishClientEnvelopes` |

**So in the default configuration, every client-facing endpoint is unauthenticated.** There is no
per-topic read authorization anywhere.

All errors collapse to one message to avoid leaking internals: `Unauthenticated`, `"invalid auth
token"`. `RequireNodeAuthInterceptor` returns `Unauthenticated`, `"node authentication required"`.

### 14.3 The interceptor chain

Built in `pkg/api/server.go`, `NewAPIServer`, outermost first:

1. **`TracingInterceptor`** (only when `--tracing.enable`) — one Datadog APM span per RPC.
2. **`ProtocolValidationInterceptor`** — allows only `connect.ProtocolGRPC` and
   `connect.ProtocolGRPCWeb`. The plain Connect protocol is rejected: `FailedPrecondition`,
   `"Connect-RPC protocol not supported, use gRPC or gRPC-Web"`.
3. **`GRPCMetricsInterceptor`** — emits `grpc_server_*` metrics, wrapping streams to count per-message
   sends and receives.
4. **`OpenConnectionsInterceptor`** — maintains `xmtp_api_open_connections_gauge{style,method}`.
5. **`LoggingInterceptor`** — logs, then **sanitizes** every error via `sanitizeError`:

| Input | Output |
| --- | --- |
| `context.DeadlineExceeded` | `DeadlineExceeded`, `request timed out` |
| `context.Canceled` | `Canceled`, `request was canceled` |
| `InvalidArgument` / `Unimplemented` / `NotFound` | message preserved |
| `Internal` | message replaced with `internal server error` |
| any other connect code | message replaced with `request has failed` |
| non-connect error | `Unknown`, `unknown error` |

This is why several bare errors listed in §6 reach the client as `"unknown error"`. §6.0 states the
same rule and applies it row by row to every error table in this document. In short: **only
`InvalidArgument`, `Unimplemented`, and `NotFound` keep their handler text**; every other code keeps
its code and loses its message.

Then per-surface, appended innermost:

| Surface | Extra interceptor | Condition |
| --- | --- | --- |
| all | `ServerAuthInterceptor` | a JWT verifier exists |
| `QueryApi` | `RateLimitInterceptor` | rate limiting built |
| `NotificationApi` | `StreamLimitInterceptor` | rate limiting built |
| `ReplicationApi` | `RequireNodeAuthInterceptor` | `--api.require-replication-node-auth` |

Client side, `pkg/interceptors/client/auth.go`, `ClientAuthInterceptor` always attaches the header, with
a token cache refreshed when within `MaxClockSkew` (2 min) of expiry, using a read-locked fast path and
a double-checked write-locked slow path.

---

## 15. Pruning and expiry enforcement

`pkg/prune/*`. `Executor.Run()` does `PruneRows()` then `DropPrunablePartitions()`.

There is **no internal scheduler** — no interval config exists in `pkg/prune` or `pkg/config/pruner.go`.
`Executor.Run()` is a single pass per invocation; scheduling is external (a separate binary run by cron
or a Kubernetes CronJob).

### 15.1 Row pruning

`pkg/prune/row_pruner.go`, `Executor.PruneRows`:

1. If `--count-deletable`, run `CountExpiredEnvelopes` first and log; exit early on 0.
2. If `--dry-run`, log and return.
3. `SelectVectorClock` and `GetPrunableCeiling`.
4. Build `deletableTables: gateway_envelopes_meta_o<id> → ceiling`, **skipping originators 0 and 1**
   ("originator is not prunable in this version of XMTPD") and any originator whose ceiling is 0
   ("No reports exist").
5. Loop up to `MaxCycles` (default 10). Each cycle, one DELETE per remaining table:

```sql
WITH to_delete AS (
  SELECT ctid FROM "gateway_envelopes_meta_o<id>"
  WHERE expiry < EXTRACT(EPOCH FROM now())::bigint
    AND originator_sequence_id < <ceiling>
  ORDER BY expiry
  LIMIT <batch_size>
  FOR UPDATE SKIP LOCKED
)
DELETE FROM "gateway_envelopes_meta_o<id>" WHERE ctid IN (SELECT ctid FROM to_delete);
```

   `FOR UPDATE SKIP LOCKED` avoids blocking on concurrently-locked rows. Blobs go away via the FK
   cascade.
6. A table drops out of the set once a cycle deletes fewer than `batch_size` rows, or on error.

**The critical detail**: `GetPrunableCeiling` is `COALESCE(MAX(end_sequence_id), 0)` over
`payer_reports WHERE submission_status IN (1, 2)` (SUBMITTED or SETTLED). **A row is only prunable once
a payer report covering it has been submitted or settled on chain** — expiry alone is not enough. So
if the payer-report pipeline stalls, nothing is ever deleted, no matter how long expired.

**[DROP the gate]** — with no payer reports, "expired" is the whole condition.

### 15.2 Partition dropping

`pkg/prune/partition_prune.go` + `get_prunable_meta_partitions()`
(`pkg/db/migrations/00022_prune-meta-partitions.up.sql`).

The function walks `pg_inherits` for leaf tables matching
`^gateway_envelopes_meta_o[0-9]+_s[0-9]+_[0-9]+$`, ranks them by `band_start DESC` per originator, keeps
only `rn > 1` (so **the newest band per originator is never dropped, even if empty**), and for each
remaining candidate runs `SELECT EXISTS (SELECT 1 FROM ... LIMIT 1)` to confirm it is empty.

The executor then derives the paired blob name via `constructBlobName` —
`gateway_envelopes_blob_o<id>_s<start>_<end>` — and drops both:

```sql
DROP TABLE IF EXISTS "gateway_envelopes_meta_o<id>_s<s>_<e>","gateway_envelopes_blob_o<id>_s<s>_<e>" CASCADE
```

Originators 0 and 1 are skipped here too. Note the blob partition's emptiness is **not** independently
checked — it is assumed to mirror the meta partition via the cascade.

### 15.3 Config

| Field | Flag | Env | Default |
| --- | --- | --- | --- |
| `MaxCycles` | `--max-prune-cycles` | `XMTPD_PRUNE_MAX_CYCLES` | 10 |
| `BatchSize` | `--batch-size` | `XMTPD_PRUNE_BATCH_SIZE` | 10 000 |
| `CountDeletable` | `--count-deletable` | `XMTPD_PRUNE_COUNT_DELETABLE` | false |
| `DryRun` | `--dry-run` | `XMTPD_PRUNE_DRY_RUN` | false |

`NewPruneExecutor` panics if `BatchSize <= 0` or `MaxCycles <= 0`.

---

## 16. Configuration

Flags use `jessevdk/go-flags` conventions; env vars are `XMTPD_<NAMESPACE>_<FIELD>`.

**This is a selected list, not an exhaustive one.** It covers the options that shape the behavior
described in this document. Contract addresses, chain plumbing, and deployment-block settings are
summarized rather than enumerated. §16.8 lists behavior-affecting options that the rest of §16 omits.

### 16.1 API — `pkg/config/options.go`, `APIOptions`

| Field | Flag | Env | Default | Effect |
| --- | --- | --- | --- | --- |
| `Enable` | `--enable` | `XMTPD_API_ENABLE` | false | serve the client API |
| `SendKeepAliveInterval` | `--send-keep-alive-interval` | `XMTPD_API_SEND_KEEP_ALIVE_INTERVAL` | **30s** | every stream's keepalive; **also the XIP-83 ping cadence, pong deadline, send-stall timeout, and flush timeout** |
| `Port` | `--port` / `-p` | `XMTPD_API_PORT` | 5050 | listen port |
| `OriginatorCacheTTL` | `--originator-cache-ttl` | `XMTPD_API_ORIGINATOR_CACHE_TTL` | **5m** | staleness window for the originator list used to fill cursors (§7.3) |
| `RequirePayerPositiveBalance` | `--require-payer-positive-balance` | `XMTPD_API_REQUIRE_PAYER_POSITIVE_BALANCE` | false | reject publishes on insufficient balance |
| `RequireReplicationNodeAuth` | `--require-replication-node-auth` | `XMTPD_API_REQUIRE_REPLICATION_NODE_AUTH` | **false** | require node JWT on all ReplicationApi methods |

`SendKeepAliveInterval` is doing a lot of work — it is the single knob behind five distinct timeouts in
`Subscribe`. A new backend should separate ping cadence from reap deadline from send-stall timeout.

### 16.2 Database — `DBOptions`

| Field | Flag | Default |
| --- | --- | --- |
| `ReaderConnectionString` | `--reader-connection-string` | — |
| `WriterConnectionString` | `--writer-connection-string` | — |
| `ReadTimeout` | `--read-timeout` | 10s (becomes `statement_timeout`) |
| `WriteTimeout` | `--write-timeout` | 10s |
| `MaxOpenConns` | `--max-open-conns` | 80 |
| `WaitForDB` | `--wait-for` | 30s |
| `NameOverride` | `--name-override` | — |

### 16.3 Rate limiting — `RateLimitOptions`

| Field | Flag | Default |
| --- | --- | --- |
| `Enable` | `--enable` | **false** |
| `T2PerMinuteCapacity` | `--t2-per-minute-capacity` | 60 |
| `T2PerHourCapacity` | `--t2-per-hour-capacity` | 1200 |
| `T2SubscribeOpensPerMinute` | `--t2-subscribe-opens-per-minute` | 10 |
| `BreakerFailureThreshold` | `--breaker-failure-threshold` | 5 |
| `BreakerCooldown` | `--breaker-cooldown` | 10s |
| `RedisCallTimeout` | `--redis-call-timeout` | 50ms |
| `TrustedProxyCIDRs` | `--trusted-proxy-cidrs` | "" |
| `T1MaxConcurrentSubscribeAll` | `--t-1-max-concurrent-subscribe-all` | 2 |
| `StreamTTL` | `--stream-ttl` | 15m |
| `StreamRefreshInterval` | `--stream-refresh-interval` | 5m |

### 16.4 Redis — `pkg/config/redis.go`

| Field | Flag | Default |
| --- | --- | --- |
| `RedisURL` | `--redis-url` | — |
| `KeyPrefix` | `--key-prefix` | `xmtpd:` |
| `ConnectTimeout` | `--connect-timeout` | 10s |

### 16.5 Payer / gateway — `PayerOptions` **[DROP]**

| Field | Flag | Default |
| --- | --- | --- |
| `PrivateKey` | `--private-key` | — |
| `Enable` | `--enable` | false |
| `NodeSelectorStrategy` | `--node-selector-strategy` | `stable` |
| `NodeSelectorPreferredNodes` | `--node-selector-preferred-nodes` | — |
| `NodeSelectorCacheExpiry` | `--node-selector-cache-expiry` | 5m |
| `NodeSelectorTimeout` | `--node-selector-connect-timeout` | 2s |
| `EnvelopePublishTimeout` | `--envelope-publish-timeout` | **30s** |
| `EnvelopePublishRetries` | `--envelope-publish-retries` | **5** |

### 16.6 Other subsystems

| Namespace | Key options | Notes |
| --- | --- | --- |
| `MlsValidationOptions` | `--grpc-address` | the MLS validation service address |
| `IndexerOptions` | `--enable` (false) | **[DROP]** |
| `SyncOptions` | `--enable` (false) | **[DROP]** |
| `PayerReportOptions` | `--enable` (false), attestation poll 1m, self period 6h, others 12h, expiry 9h / 18h | **[DROP]** |
| `MetricsOptions` | `--enable` (false), address `127.0.0.1`, port 8008 | |
| `DebugOptions` | `--enable` (false), port 6060 (pprof) | |
| `TracingOptions` | `--enable` (false) | Datadog APM |
| `ReflectionOptions` | `--enable` (false) | gRPC reflection |
| `LogOptions` | `--log-level` (INFO), `--log-encoding` (console/json) | |
| `SignerOptions` | `--private-key` | node identity key |
| `AppChainOptions` **[DROP]** | RPC/WSS URLs, chain id 31337, `--max-chain-disconnect-time` 60s, `--backfill-block-page-size` 500, `--max-blockchain-payload-size` **200 000**, contract addresses | |
| `SettlementChainOptions` **[DROP]** | RPC/WSS URLs, chain id 31337, `--max-chain-disconnect-time` 300s, `--node-registry-refresh-interval` **60s**, `--rate-registry-refresh-interval` 300s, contract addresses | |
| `MigrationServer/ClientOptions` **[DROP]** | batch size 1000, process interval 10s, `--from-node-id` 100 | |
| `PruneOptions` | see §15.3 | |

### 16.7 Hard-coded values that arguably should be config

| Value | Where |
| --- | --- |
| Publish worker batch 100, ticker 1s, 3 deadlock retries | `pkg/api/message/publish_worker.go` |
| Subscribe poll 100 ms, 1000 rows, listener buffer 1024 | `pkg/api/message/subscribe_worker.go` |
| Cursor updater poll 100 ms | `pkg/api/metadata/cursor_updater.go` |
| `SubscribeSyncCursor` keepalive 30 s | `pkg/api/metadata/service.go` |
| Wait-for-publish 30 s / 10 ms | `pkg/api/message/service.go` |
| Every XIP-83 limit (§6.3) | `pkg/api/message/subscribe.go` |
| Every query limit (§6.2) | `pkg/api/message/service.go` |
| Partition band width 1 000 000, fill threshold 70%, check interval 30 min | `pkg/db/types.go`, `pkg/db/worker/worker.go` |
| Backoff 50 ms / 300 ms / 2 s | throughout |
| Identity update validation history limit 256 | `identity_update_storer.go` |

### 16.8 Behavior-affecting options omitted from the tables above

The tables in §16.1–16.6 are a selected list. These options also change behavior and are absent from
them:

| Option | Flag / env | Default | Effect |
| --- | --- | --- | --- |
| `ContractsOptions.ConfigFilePath` | `--contracts.config-file-path` / `XMTPD_CONTRACTS_CONFIG_FILE_PATH` | — | loads the contracts config from a JSON file on disk |
| `ContractsOptions.ConfigJSON` | `--contracts.config-json` / `XMTPD_CONTRACTS_CONFIG_JSON` | — | supplies the same config inline as JSON |
| `ContractsOptions.Environment` | `--contracts.environment` / `XMTPD_CONTRACTS_ENVIRONMENT` | — | selects a named deployed environment's contracts config |
| `SettlementChainOptions.BackfillBlockPageSize` | `--settlement-chain.backfill-block-page-size` | **500** | page size for settlement-chain backfill (the app-chain twin is documented in §16.6; this one was missing) |
| `AppChainOptions.DeploymentBlock` | `--app-chain.deployment-block` | 0 | first block the app-chain indexer reads |
| `MigrationServerOptions.PayerPrivateKey` | `--migration.payer-private-key` | — | key used to sign payer envelopes during backfill |
| `MigrationServerOptions.NodeSigningKey` | `--migration.node-signing-key` | — | key used to sign originator envelopes during backfill |
| `MigrationServerOptions.Namespace` | `--migration.namespace` / `XMTPD_MIGRATION_DB_NAMESPACE` | `""` | namespace applied to migrated rows |
| `MigrationServerOptions.LowerLimits` | `--migration.lower-limits` / `XMTPD_MIGRATION_LOWER_LIMITS` | — | JSON map of migration source → lower sequence limit; skips everything below it |
| `MigrationServerOptions.ReaderTimeout` | `--migration.reader-timeout` | 10s | read timeout against the v2 source database |

The three `ContractsOptions` fields matter most: they are three mutually-substitutable **sources** for
the same contract configuration (file, inline JSON, or named environment), and which one is set
changes where every contract address comes from. All are **[DROP]** for a chainless backend, but a
reader auditing today's behavior needs to know the config can arrive three ways.

`LowerLimits` is the one migration option with a real correctness consequence: it silently excludes
source rows below the named sequence, so a mis-set value produces a backfill that looks complete and
is not.

---

## 17. Metrics

Registered by `pkg/metrics/metrics.go`, `registerCollectors`, served on `--metrics.metrics-port` (8008)
with OpenMetrics enabled. The `ratelimiter` package registers separately via `ratelimiter.Register`.
Names only; labels in parentheses.

**API** (`pkg/metrics/api.go`): `xmtp_api_open_connections_gauge` (style, method),
`xmtp_api_incoming_node_connection_by_version_gauge` (version),
`xmtp_api_node_connection_requests_by_version_counter` (version),
`xmtp_api_failed_grpc_requests_counter` (code), `xmtp_api_stage_envelope_seconds`,
`xmtp_api_wait_for_gateway_publish_seconds`, `xmtp_api_staged_envelope_processing_delay_seconds`,
`xmtp_api_outgoing_envelopes_total` (method).

**gRPC** (`pkg/metrics/grpc.go`), deliberately mirroring the grpc-ecosystem names:
`grpc_server_started_total`, `grpc_server_handled_total` (+ `grpc_code`),
`grpc_server_msg_received_total`, `grpc_server_msg_sent_total`, `grpc_server_handling_seconds` — all
labelled `grpc_type, grpc_service, grpc_method`.

**Sync** (`pkg/metrics/sync.go`): `xmtp_sync_originator_sequence_id` (originator_id),
`xmtp_sync_messages_received_error_count`, `xmtp_sync_messages_received_count`,
`xmtp_sync_outgoing_sync_connections`, `xmtp_sync_failed_outgoing_sync_connections`,
`xmtp_sync_failed_outgoing_sync_connections_counter`, `xmtp_sync_subscribe_rpc_total` (rpc,
originator_id).

**Database** (`pkg/metrics/dbmetrics.go`): `db_query_duration_seconds` (query, op),
`db_query_errors_total` (query, op). The `query` label is extracted from the `-- name: X` sqlc comment
by regex; transaction-control statements are filtered out.

**Gateway/payer** (`pkg/metrics/payer.go`) **[DROP]**: `xmtp_gateway_publish_duration_seconds`,
`xmtp_gateway_lru_nonce`, `xmtp_gateway_failed_attempts_to_publish_to_node_via_banlist`,
`xmtp_gateway_messages_originated`, `xmtp_gateway_get_nodes_available_nodes`.

**Indexer** (`pkg/metrics/indexer.go`) **[DROP]**: `xmtp_indexer_log_streamer_logs`,
`..._current_block`, `..._max_block`, `..._block_lag`, `xmtp_indexer_retryable_storage_error_count`,
`..._get_logs_duration`, `..._get_logs_requests`, `xmtp_indexer_log_processing_time_seconds`,
`xmtp_indexer_bytes_indexer`.

**Blockchain** (`pkg/metrics/blockchain.go`) **[DROP]**:
`xmtp_blockchain_wait_for_transaction_seconds`, `xmtp_blockchain_publish_payload_seconds`,
`xmtp_blockchain_broadcast_transaction_seconds`, `xmtp_blockchain_oracle_gas_price`,
`..._gas_price_updates_total`, `..._gas_price_default_fallback_total`,
`..._gas_price_last_update_timestamp_unix`.

**Migrator** (`pkg/metrics/migrator.go`) **[DROP]**: 13 metrics prefixed `xmtp_migrator_*`.

**Rate limiting** (`pkg/ratelimiter/metrics.go`, `stream_metrics.go`):
`xmtpd_rate_limit_decisions_total` (service, method, tier, outcome ∈ {bypassed, failed_open, denied,
allowed}), `xmtpd_rate_limit_circuit_breaker_state`, `xmtpd_rate_limit_circuit_breaker_trips_total`,
`xmtpd_stream_limit_decisions_total` (service, outcome), `xmtpd_stream_limit_active_streams`.

Plus standard `collectors.NewProcessCollector` and `NewGoCollector`.

**Bucket sets** (`pkg/metrics/buckets.go`): `SubSecondBuckets` — 44 points from 5 ms to 1 s, dense
(10 ms) from 40–300 ms and coarser (50 ms) from 350 ms–1 s. `Precision50msBucket` — 50 ms steps to 1 s,
then 2.5/5/10 s.

**The gap worth noting**: there is **no metric for subscription lag** (how far behind a live subscriber
is), **no metric for listeners reaped due to a full channel** (only an APM span), and **no metric for
XIP-83 wave counts, mutate rates, or pending-buffer occupancy**. A new backend should instrument those.

---

## 18. Limits, one table

| Limit | Value | Constant / source |
| --- | --- | --- |
| **Transport** | | |
| Max gRPC message, **node APIs only** (`ReplicationApi`, `QueryApi`, `PublishApi`, `NotificationApi`, `MetadataApi`) | 25 MiB read and send | `constants.GRPCPayloadLimit`, applied in `pkg/server/server.go`, `registrationFunc` |
| Max gRPC message, **gateway** (`PayerApi`, `GatewayApi`) | **none** — no `WithReadMaxBytes` / `WithSendMaxBytes`; Connect treats unset as unlimited | `pkg/gateway/builder.go` |
| HTTP idle timeout | 5 min | `pkg/api/server.go` |
| HTTP read header / read timeout | 10 s / 30 s | `pkg/api/server.go` |
| **Publish** | | |
| Max envelopes per publish | **unbounded** (only the 25 MiB cap) | — |
| Max blockchain payload | 200 000 bytes | `--app-chain.max-blockchain-payload-size` |
| Retention days | 2 ≤ n ≤ 365, or exactly `MaxUint32` | `Service.validateExpiry` |
| Default retention | 60 days | `constants.DefaultStorageDurationDays` |
| Publish worker batch | 100 | `numRowsPerBatch` |
| Publish wait timeout | 30 s, polled at 10 ms | `Service.waitForGatewayPublish` |
| Payer publish retries / timeout | 5 / 30 s | `--payer.envelope-publish-*` |
| **Query** | | |
| Default page size | 1000 (0 means 1000) | `maxRequestedRows` |
| Max page size | 1000 (larger silently clamped) | `maxRequestedRows` |
| Max topics + originators | 10 000 | `maxQueriesPerRequest` |
| Max topic length | 128 bytes (`QueryEnvelopes` / `SubscribeTopics` **only**; XIP-83 `Subscribe` has **no** maximum) | `maxTopicLength` |
| Max cursor entries | 100 | `maxVectorClockLength` |
| Max inbox-id requests | 1000 | `maxInboxIdsPerRequest` |
| `GetNewestEnvelope` topics | **unbounded** | — |
| `GetPayerInfo` addresses | **unbounded** | — |
| **SubscribeTopics** | | |
| Max filters | 10 000 | `maxTopicFilters` |
| Catch-up chunk | 500 topics | `maxTopicsPerChunk` |
| Catch-up page | 500 rows, min 10 per (topic, originator) | `topicPageLimit`, `CalculateRowsPerEntry` |
| **Subscribe (XIP-83)** | | |
| Active topics per stream | 1 000 000 — counts **live** topics only; `history_only` topics never enter `sess.topics` and bypass this cap | `maxActiveSubscribeTopics` |
| Max topic length | **none** — 2-byte minimum and kind check only | `topic.ParseTopic` |
| Adds per Mutate | 100 000 (pre-dedup) | `maxMutateAdds` |
| Cursor entries per Mutate | 1 000 000 (pre-dedup) | `maxMutateCursorEntries` |
| In-flight waves | 256 | `maxInflightSubscribeWaves` |
| Pending buffer | 64 MiB | `maxSubscribePendingBytes` |
| Frame size target | 2 MiB | `maxSubscribeFrameBytes` |
| Send / catch-up queue depth | 8 / 16 | `subscribeSendQueueDepth`, `subscribeCatchUpQueueDepth` |
| Mutation rate | **unlimited** | — |
| Client ping rate | **unlimited** | — |
| Per-RPC admission on `Subscribe` (opens) | **none** — the opens limiter covers `SubscribeTopics` only | `pkg/interceptors/server/rate_limit.go` |
| **Live delivery** | | |
| Listener channel depth | 1024 | `subscriptionBufferSize` |
| Poll interval / page | 100 ms / 1000 rows | `SubscribeWorkerPollTime`, `subscribeWorkerPollRows` |
| Keepalive interval | 30 s | `--api.send-keep-alive-interval` |
| **Rate limiting** (default off) | | |
| Query tokens | 60/min, 1200/h, cost `ceil(sqrt(topics))` — applied to the four unary `QueryApi` methods only | `RateLimitOptions` |
| Subscribe opens | 10/min — **`SubscribeTopics` only**; XIP-83 `Subscribe` is unlimited | `T2SubscribeOpensPerMinute` |
| Concurrent firehose streams | 2 per IP | `T1MaxConcurrentSubscribeAll` |
| Breaker | 5 failures, 10 s cooldown, 50 ms Redis timeout | `RateLimitOptions` |
| **Storage** | | |
| Partition band width | 1 000 000 | `GatewayEnvelopeBandWidth` |
| Partition pre-create threshold | 70% at 30 min checks | `DefaultFillThreshold`, `DefaultCheckInterval` |
| Prune batch / cycles | 10 000 / 10 | `PruneConfig` |
| Identity update validation history | 256 rows | `identity_update_storer.go` |
| **Auth** | | |
| JWT lifetime / max lifetime | 1 h / 2 h | `tokenDuration`, `maxTokenDuration` |
| Clock skew | 2 min | `authn.MaxClockSkew` |

---

## 19. Recommendations for the new backend

### 19.1 Keep these, nearly as-is

1. **The serialized-staging advisory lock.** `pg_advisory_xact_lock(hashtext(...))` around id assignment
   is the mechanism that makes `BIGSERIAL` order equal commit order. Whatever shape the write path
   takes, it must guarantee that **sequence ids become visible in order**. Without it every cursor in
   the system is wrong.
2. **The `Fatal` on out-of-order rows.** Crashing beats serving a gap. Keep an equivalent assertion.
3. **The trigger-maintained high-water mark.** A statement-level `AFTER INSERT` trigger with a
   monotonic guard costs nothing and cannot be forgotten by application code.
4. **The meta/blob split.** Narrow hot index, wide cold payload, joined only for surviving rows.
5. **The whole XIP-83 control protocol.** Frames, waves, `mutate_id` tagging, the seam, `TopicsLive` /
   `CatchupComplete`, two independent liveness timers, `drainPendingRequests` before reaping,
   gate-before-fetch, the bounded `send`. See `xip-83.md` for the detail.
6. **`GetSyncCursor` / `SubscribeSyncCursor`.** "Am I caught up?" is the most useful metadata call there
   is.
7. **The keepalive-payload lesson.** Every stream needs an application-level heartbeat, because
   terminating proxies answer transport pings.
8. **`address_log` as a projection with monotonic upsert guards.**
9. **The `SubscribeAllEnvelopes` firehose plus a concurrency cap.**
10. **Client-IP extraction with trusted-proxy peeling and IPv6 /64 collapsing**, including the
    `"invalid"` sentinel so bad input cannot pollute the keyspace.

### 19.2 Simplify dramatically

| Today | Tomorrow |
| --- | --- |
| `Cursor` = `map<uint32,uint64>` | a scalar `u64` |
| `FillMissingOriginators` + a 5-minute TTL originator cache | deleted; the class of bug goes with it |
| Wave ceilings as a per-originator vector | one `MAX(sequence_id)` scalar |
| `maxMutateCursorEntries` = 1 000 000 | deleted |
| Topics × originators LATERAL cross product | one keyset range scan `WHERE topic = ANY($1) AND sequence_id > $2 ORDER BY sequence_id LIMIT n` |
| Two-level LIST+RANGE partitioning, band arithmetic, three generations of plpgsql, a reader/writer advisory lock, savepoint-retry | at most single-level **time** partitioning, pre-created by cron |
| "Ordered per originator" | a genuine **total order** |
| Four levels of byte-nesting | at most two |
| `depends_on` causal vector | probably nothing — one DB gives real ordering |
| Client resume state = a vector | one `u64` |

The single most valuable consequence: with one sequence, XIP-83's requirement 4 "live total order"
becomes a real total order across all topics, and the v3 floor rules (a lower-cursor re-add restarting
catch-up) become expressible again — the d14n binding had to give those up because vector cursors are
only partially ordered.

### 19.3 Drop entirely

Payer signatures and the payer service; `target_originator`; originator ids, signatures, and the node
registry; blockchain proofs, the indexer, reorg handling, and `blockchain_messages`; fees, `payers`,
`unsettled_usage`, `payer_ledger_events`, `originator_congestion`, `payer_reports`, and their
attestations; the nonce table; payer-report and payer-report-attestation topic kinds and the
`IsReserved` concept; the node-to-node sync package; the v2→v3 migrator; `MisbehaviorApi`;
`SubscribeOriginators`; the node JWT scheme as written (it resolves keys from the on-chain registry).

### 19.4 Fix these

1. **Publish idempotency.** There is none. A retried publish creates a duplicate with a new sequence
   id. Add a client-supplied idempotency key or a content-hash unique index.
2. **`GetNewestEnvelope` ordering.** It sorts by `gateway_time`, and the query file says so:
   *"sorting by gateway time can lead to wrong results, this query needs to be redone."* With one
   sequence, `DISTINCT ON (topic) ... ORDER BY topic, sequence_id DESC` is correct and cheaper.
3. **Expiry is not enforced on read.** Expired rows are served until the pruner deletes them — and the
   pruner is gated on payer reports, so a stalled pipeline means expired data is served indefinitely.
   Either filter on read or make pruning depend only on expiry.
4. **Legacy streams close cleanly when a consumer falls behind.** `SubscribeTopics`,
   `SubscribeEnvelopes`, and `SubscribeOriginators` all return `nil` — a successful-looking close — when
   the worker reaps their listener. Only XIP-83 returns `Aborted`. Always signal data loss.
5. **Oversized envelopes are silently skipped** on both the batching and the XIP-83 paths. Cap envelope
   size at publish so the row cannot exist.
6. **Unmarshal failures silently shrink pages.** A corrupt row is logged and skipped; the client sees a
   gap with no signal.
7. **Validation coverage.** Welcome messages are entirely unvalidated; group messages are never sent to
   the MLS validator (`ValidateGroupMessages` has no caller). Decide deliberately.
8. **String-matched error taxonomy** in `shouldRetryValidationError`. Its own comment calls it fragile.
   Return typed errors from the validation boundary. In Rust this is nearly free.
9. **No mutation-rate or client-ping-rate limit** on `Subscribe` (XIP-83 requirement 8 asks for both).
10. **Rate limiting charges only stream opens**, never lifetime (deferred in xmtpd#1957).
11. **`SendKeepAliveInterval` is overloaded** as five different timeouts. Split them.
12. **Unbounded request sizes** on `GetNewestEnvelope` and `GetPayerInfo`, and unbounded envelope counts
    on publish.
13. **`GetInboxIds` result scatter is an O(n²) nested loop** at a 1000-address cap. One map.
14. **`GetInboxIds` does not normalize addresses** while `GetPayerByAddress` does. Pick one.
15. **Identity update validation reads only 256 prior updates.** Either paginate or document the bound.
16. **No read authorization anywhere.** If the new backend is self-hosted and multi-tenant, decide this
    up front. XIP-83 requirement 7 binds it: authorize **per topic**, never per connection, or the
    multiplexing gateway use case breaks.

### 19.5 Reconsider

- **Polling versus `LISTEN`/`NOTIFY`.** xmtpd polls every 100 ms per originator because no single node
  sees all writes. One Postgres makes `LISTEN`/`NOTIFY` viable, cutting tail latency and removing a
  poller per originator. Keep a timer fallback — `NOTIFY` is not delivered across a replica.
- **Backpressure policy.** "Drop the subscription at 1024 buffered batches" is defensible under XIP-83
  (the client reconnects from durable cursors) but should be a deliberate, configurable choice, not an
  inherited constant.
- **Whether `staged_originator_envelopes` survives.** It exists to separate "accepted" from "sequenced
  and durable" across a node boundary. With one backend, a single transaction that assigns the sequence
  and inserts the row may be enough — but the staging table is also what makes the publish path's
  batching and the ordering lock cheap. Measure before removing it.
- **Whether the meta/blob split is still worth it.** It is a real win at scale, and a real complexity
  cost. Benchmark against a single table with the payload as a trailing column.

### 19.6 The five things that matter most

1. **Ordering is everything, and it is enforced by one advisory lock plus one sequential writer.**
   `pg_advisory_xact_lock(hashtext('staged_originator_envelopes_sequence'))` around id assignment, then
   a single publish worker draining `ORDER BY id ASC ... FOR UPDATE` in batches of 100. Two places
   `Fatal` if the invariant breaks. Reproduce this guarantee or nothing downstream is sound.

2. **Identity updates are ordered by a smart contract, not by xmtpd.** Every node is a replica of a
   chain-produced log; the contract assigns the `SequenceId` that becomes `association_sequence_id` and
   thus the answer to "which inbox owns this address." Removing the chain means the new backend must
   sequence identity updates itself — which one Postgres does better, but it is a genuine
   responsibility transfer, not a deletion.

3. **The cursor is a per-originator vector, and that infects everything.** `maxVectorClockLength = 100`,
   `maxMutateCursorEntries = 1_000_000`, `FillMissingOriginators`, the TTL originator cache, wave
   ceiling vectors, the topics × originators LATERAL cross product, and XIP-83's inability to express
   "below the floor" all exist because of it. Collapsing to a scalar `u64` deletes an entire complexity
   class and makes ordering genuinely total.

4. **The limits are the design.** Page size 1000 (0 means 1000, larger silently clamped), 10 000 topics,
   128-byte topics, 100 cursor entries, 25 MiB messages, 2 MiB frames, 1 M subscribe topics, 100 K adds
   per Mutate, 256 in-flight waves, 64 MiB pending, 1024-deep listener channels, 60-day default
   retention within 2–365 bounds, 500-row catch-up pages, 100 ms polling. Publish envelope count is
   unbounded — fix that.

5. **XIP-83's `Subscribe` is the best code in the repository and should be ported closely.** The single-
   writer model, the two independent liveness timers, gate-before-fetch, the ceiling-pinned merged wave
   scan, per-frame `mutate_id` tagging, the fold-and-flip seam, and `drainPendingRequests` before reaping
   each solve a real, hard-won bug. The comments explaining *why* are as valuable as the code. Read
   `xip-83.md` alongside `pkg/api/message/subscribe.go` before rewriting any of it.

---

## Review status

**Review thread id**: `01a0624f-3cef-79c3-bf49-6859069fe45e` (adversarial review, model `gpt-5.6-sol`,
read-only sandbox, cwd `/Users/nickmolnar/code/xmtp/xmtpd`). Verdict: ISSUES. Every finding below was
re-checked against the cited source before any text in this document was changed.

### Findings against this document (A)

| Finding | Applied or rejected | Note |
| --- | --- | --- |
| **blocker** — endpoint error tables present handler messages as wire-visible messages; the logging interceptor rewrites most of them | **applied** | Verified `pkg/interceptors/server/logging.go`, `sanitizeError` (its own doc comment states the rule) and `pkg/api/server.go`, `NewAPIServer` (the interceptor is appended to `serverInterceptors` and passed to `cfg.RegistrationFunc`, so it wraps every service). Added §6.0 as a marked subsection at the top of the endpoint sections, and added a **Wire message** column to every error table in §6, §13.7. Only `InvalidArgument`, `Unimplemented`, `NotFound` keep handler text; `Internal` → `internal server error`; all other connect codes → `request has failed`; bare errors → `Unknown` / `unknown error`. |
| **major** — §6.1 key-package errors listed as `Internal`; `preprocessPayerEnvelopes` stringifies them and the outer handler returns `InvalidArgument` | **applied** | Verified `pkg/api/message/service.go`: `preprocessPayerEnvelopes` accumulates `errs []string`; the caller wraps the joined result in `connect.NewError(connect.CodeInvalidArgument, fmt.Errorf("error processing payer envelopes:%w", err))`. `validateKeyPackage`'s `CodeInternal` is discarded. Added an explanatory subsection and marked the affected rows. |
| **major** — §6.1 omits base-fee and congestion-fee error paths | **applied** | Verified `pkg/api/message/service.go`, `preprocessPayerEnvelopes`: both `CalculateBaseFee` and `CalculateCongestionFee` return a plain error immediately (they do **not** accumulate), which the caller turns into `InvalidArgument`. Added both rows plus a note that a fee failure aborts the whole request and can mask an internal fault. |
| **major** — §6.1 response and stored envelope can differ | **applied** | Verified two independent `SignStagedEnvelope` calls (handler in `service.go`; worker in `publish_worker.go`, `prepareSingleEnvelope`, which **recomputes** both fees at `stagedEnv.OriginatorTime` and zeroes them for reserved topics) and `pkg/registrant/registrant.go`, `SignStagedEnvelope`, which reads `time.Now()` for `ExpiryUnixtime`. Added a comparison table: topic, payer bytes, originator, sequence, and originator time match; fees, expiry, unsigned bytes, and signature can differ. |
| **major** — "Max gRPC message 25 MiB" applies only to node APIs; the gateway installs no cap | **applied** | Verified `pkg/server/server.go`, `registrationFunc` (`handlerOpts` and `queryHandlerOpts` both carry `WithReadMaxBytes`/`WithSendMaxBytes`) against `pkg/gateway/builder.go`, which registers `PayerApi` and `GatewayApi` with `connect.WithInterceptors(...)` only. Connect treats unset as unlimited. Corrected §5 and split the §18 row in two. |
| **major** — §13.2 "all QueryApi methods" omits bidi `QueryApi.Subscribe` | **applied** | Verified `pkg/interceptors/server/rate_limit.go`: `QueryApiMethodFromProcedure` is a closed switch over four unary/streaming procedures that does **not** include `Subscribe`; `WrapStreamingHandler` further narrows to `MethodSubscribeTopics`. Rewrote the §13.2 table and added an explicit "exactly which procedures are limited" subsection. XIP-83 `Subscribe` has no open, mutation, ping, or lifetime limit. |
| **major** — §6.8 positional `GetNewestEnvelope` breaks on duplicate topics | **applied** | Verified `pkg/api/message/service.go`, `GetNewestEnvelope`: `originalSort[string(topic)] = idx` keeps only the last index per topic, and `SelectNewestFromTopics` is `DISTINCT ON (m.topic)`, returning one row per distinct topic. Documented that every duplicate slot but the last is left `nil`. |
| **major** — §8 names `migration_tracker` and `migration_dead_letter_box` but does not define them | **applied** | Read `pkg/db/migrations/00011_add-migration-tracker.up.sql`, `00013_add-commit-messages-migration.up.sql`, `00014_add-dead-letter-box.up.sql`. Added §8.7a with full DDL: both tables, all five seed rows (four in 00011 plus `commit_messages` in 00013), both indexes (one partial, `WHERE retryable = TRUE`), and both plpgsql functions with their `hashtext` advisory lock. |
| **major** — §5 / §11.2 generated-service list is incomplete and the "no commit-log RPC" claim is false | **applied** | Verified `pkg/proto/mls/api/v1/apiv1connect/mls.connect.go` defines `BatchPublishCommitLog` and `BatchQueryCommitLog`, plus generated-only `IdentityApi`, v1 `MessageApi`, and `D14nMigrationApi`. Added a table of generated-but-unregistered services to §5 and rewrote the §11.2 commit-log claim: the RPCs exist in the vendored protos; xmtpd registers no handler, has no table, and does no validation. |
| **major** — §6.13 omits that one DB read error permanently stops the cursor updater | **applied** | Verified `pkg/api/metadata/cursor_updater.go`, `DBBasedCursorUpdater.start`: `if err != nil { return }` with only a `TODO`, exiting the goroutine. Documented the blast radius: `GetSyncCursor`, `SubscribeSyncCursor` (which keeps sending a healthy-looking stale keepalive), and `validateClientInfo`'s `depends_on` checks, which begin rejecting valid publishes. |
| **major** — §6.3 lists ceiling and wave-scan failures as `Internal`; `handleCatchUp` re-wraps them as `Unavailable` | **applied** | Verified the helpers build `CodeInternal` but run on the fetcher goroutine; `runSubscribeCatchUp` puts the error in `catchUpBatch.err` and `handleCatchUp` returns `connect.NewError(connect.CodeUnavailable, fmt.Errorf("catch-up failed: %w", b.err))`. Corrected both rows and added the quoted code with an explanation. |
| **major** — §14.1 missing `exp` or `iat` is a nil dereference, not a rejection | **applied** | Verified `pkg/authn/verifier.go`, `validateExpiry`, dereferences `issuedAt.Time` and calls `exp.Before(...)` immediately after `err`-only checks; `golang-jwt/jwt/v5` `RegisteredClaims` getters return `(nil, nil)` for an absent claim. Documented that a correctly signed token missing either claim panics rather than returning `Unauthenticated`. |
| **minor** — "Every handler begins with the same nil guard" is false | **applied** | Verified `GetNodes` (`pkg/api/payer/service.go`), `GetSyncCursor`, `SubscribeSyncCursor` (parameter is `_`), and `GetVersion` (`pkg/api/metadata/service.go`) have no `req.Msg` check, and bidi `Subscribe` has no unary request object. Rewrote the §6 preamble. |
| **minor** — §7.1 `gateway_time` "used only by GetNewestEnvelope" | **applied** | Verified the `u_prep` and `c_prep` CTEs in `pkg/db/migrations/00023_rename-envelope-blobs.up.sql` bucket it into `minutes_since_epoch` for `unsettled_usage` and `originator_congestion`, and `00018_add_latest_envelopes.up.sql` stores `MAX(gateway_time)` on `gateway_envelopes_latest`. Replaced the claim with a three-row table of its actual uses, keeping the true part (it is not a normal read-order key). |
| **minor** — §6.3 does not state that the 128-byte topic limit is absent from XIP-83 | **applied** | Verified `handleMutate` calls only `topic.ParseTopic` (2-byte minimum, kind check) and never `maxTopicLength`. Added a note after the §6.3 error table and rows in the §6.3 and §18 limits tables. |
| **minor** — §16 omits behavior-affecting options | **applied** | Verified `pkg/config/options.go` (`ContractsOptions` config source/environment, `SettlementChainOptions.BackfillBlockPageSize`, `AppChainOptions.DeploymentBlock`) and `pkg/config/migrator.go` (keys, `Namespace`, `LowerLimits`, `ReaderTimeout`). Labelled §16 a selected list and added §16.8 with the omitted options. |

### Findings against the companion document (B)

The XIP-83 findings are recorded in `xip-83.md`'s own Review status section. Two of them changed text
in **this** document as well, because the two pages describe the same handler: the catch-up
`Unavailable` mapping (§6.3) and the absence of per-RPC admission on `Subscribe` (§13.2, §18).

### Findings rejected

**None.** All 22 findings were confirmed correct against the cited source. Nothing was rejected.

### Residual risk

The corrections above were each verified against the specific file and function the review cited, and
the reviewer separately confirmed more than thirty of this document's existing citations plus all six
"gaps found in xmtpd" claims. The residual risk is therefore concentrated in what neither pass
examined rather than in what was corrected. Three areas remain thin. First, the review sampled
citations rather than checking all of them, so sections it did not sample — most of §10 beyond the
identity-update flow, §12's worker internals, §17's metric names, and the §19 recommendations — carry
their original, unaudited confidence. Second, the new **Wire message** columns were derived by
applying `sanitizeError`'s documented rule to each handler code rather than by observing traffic; a
handler that wraps an error in a second `connect.NewError` on a path not read here could produce a
different final code, and the rule is only as good as the code it was read from. Third, this document
describes one commit (`822ddc95`); the tree is under active development, and the XIP-83 handler in
particular is new enough that its error mapping and limits are likely to move. Anyone relying on an
exact message, code, or limit should re-verify it against the current source before encoding it in a
client or a test.
