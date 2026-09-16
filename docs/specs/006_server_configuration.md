# 006: Server Configuration

Status: approved, revision 4. Drafted 2026-09-15 from the owner's answers to the question list in Ref `GvJUN6bEoPtEhnNp`; revised the same day after an adversarial review and the owner's review comments.

This spec states the `ConfigurationService.GetConfiguration` RPC, the backend configuration it publishes, and how the client fetches, stores, and applies that configuration. It reverses the v1 decision in spec 001 §14 and ARC-002 that the backend serves no metadata endpoint. Spec 001 states the wire package and error contract. Spec 004 keeps ownership of the stream keepalive.

Requirements are numbered `CFG-nnn` and use EARS form. "Backend" is the server. "Client" is libxmtp. "SDK" is a language binding on top of the client. "App" is the developer's program that uses an SDK. "Stored copy" is the configuration row in the client database. "Snapshot" is the in-memory configuration a client reads for its life.

## 1. Summary

The backend publishes one read-only configuration message on an unauthenticated RPC. It carries the backend identifier, the backend and minimum client versions, an auth summary, retention periods, a curated set of limits, MLS policy, and the accepted smart contract wallet chains. The client fetches it when a database has no stored copy, stores it, and reads the stored copy at every later build. A worker refreshes the stored copy every hour. The client records the backend URL with the stored copy, so a database pointed at a different backend URL fetches again at build and fails when the identifier differs. Operators set the rules for their network in the backend configuration file instead of clients carrying hardcoded values.

## 2. Goals and non-goals

Goals:

- Operators set client policy such as group size, installation count, and commit-log use in one place: the backend configuration file. The backend publishes these values; it does not enforce them.
- The client pre-validates publishes, queries, lookups, and smart contract wallet signatures against the limits and chains the backend enforces, so a rejected call fails before any network round trip.
- A client database is bound to one backend by a stable identifier, not by a host name. A database reused against a different backend, for example through an explicit database path, fails at build.
- An operator declares the minimum libxmtp version. A client below it refuses to build, and a running client that learns of a raised minimum stops.
- Apps read the full configuration as a typed object, before and after client creation.
- Rust tests run the client against any configuration without a matching backend.

Non-goals:

- Server-side enforcement of `min_libxmtp_version`. That is a separate project. Its recorded decisions are in §9 so they are not lost.
- Server-side enforcement of group size or installation count. The backend has no group model.
- Live reload of configuration values inside a running client. The snapshot is read once at build. The client keeps exactly two live latches, backend mismatch and version too old, and they only stop the client.
- Exposing keepalive or pong timing. Spec 004 STR-001 keeps `keepalive_interval_ms` in `Started`.
- Exposing the JWKS URL, chain RPC URLs, public key material, auth leeway, or JWKS refresh timing.
- Deriving key package rotation, welcome rescan, or local pruning from retention. Retention is informational in this revision.
- A per-app minimum version.
- Databases created before this change. There are none that matter.

## 3. Context

Catalyst: owner request of 2026-09-15 and the answered question list, Ref `GvJUN6bEoPtEhnNp`.

Related specs: 001 §11 limits, §13 transport, §14 out of scope; 002 ARC-001 no instance affinity, ARC-002 services on the port, §8 configuration, ARC-095 and ARC-096 log classes, ARC-098 telemetry; 003 SEC-005 trusted network; 004 STR-001 and STR-053 keepalive, STR-076 verifier retryability.

Facts that shape the design:

- Every `[limits]` default is a shared constant. The backend config reads it as a default and the client chunker reads it as a hard value. The backend enforces the configured value, never the constant, and today it accepts configured request and response caps above 25 MiB.
- Group size and installation count are client-only checks that run after the client has read remote identity state. The backend cannot enforce them.
- Auth is on today when the `[auth]` table is present. There is no `enabled` flag. The auth layer exempts one path prefix, the gRPC health service. The client auth middleware attaches a credential to every unary call before it looks at the path.
- The client sends `x-libxmtp-version` on every request. The backend does not read it. The workspace version is a prerelease string such as `1.12.0-dev`.
- A client with a stored identity makes no network call at build. A client with no stored identity calls `get_inbox_ids` first.
- gRPC-Web serves unary calls on wasm, so the RPC works on every target.
- The client database has one baseline migration and no generic key-value table. The client transport decodes every response into a generated message, so unknown fields are dropped before the client sees them.
- Phase 4.3 auth has shipped on `self-hosted`.

Impact area: backend configuration and validation, backend service registration and auth exemption, backend telemetry, the wire proto, the client transport and auth middleware, the client builder, the client chunker and stream policy, group membership and identity checks, the smart contract wallet verifier, the client database schema, the client worker framework, the three bindings, the four SDKs, the JSON schema, the operator docs, and the deploy guides.

Existing behaviour that must not change is stated in §12.1.

Decided in review on 2026-09-15: the version comparison uses major, minor, and patch only and ignores the prerelease tag, so a `1.12.0-dev` client satisfies a `1.12.0` minimum.

Decided in review on 2026-09-15: the backend does not return its identifier on every response. Identifiers almost never change. The risk to cover is one database reused across backend instances and URLs through an explicit database path, so the client records the backend URL with the stored copy and fetches again at build when the URL changes (CFG-055).

## 4. Backend configuration

### 4.1 New and changed fields

| Field | Type | Default | Rule |
| --- | --- | --- | --- |
| `server.identifier` | string | none, required | 1 to 256 bytes of valid UTF-8 with no whitespace and no control characters. Documented convention: reverse-DNS such as `org.xmtp.dev`. |
| `server.min_libxmtp_version` | string | none: any client version may connect | A semantic version. Only major, minor, and patch are compared. |
| `auth.enabled` | bool | none, required when `[auth]` is present | `false` serves without auth and ignores the other auth fields. `true` applies the existing auth rules. |
| `mls.max_group_members` | integer | 250 | 1 to 65535. Advisory: the backend does not enforce it. |
| `mls.max_installations_per_inbox` | integer | 10 | 1 to 65535. Advisory. |
| `mls.commit_log_enabled` | bool | true | Tells clients whether to write and read the commit log. |

- CFG-001: WHEN the configuration file has no `server.identifier` THE BACKEND SHALL refuse to start with an error that names the field.
- CFG-002: WHEN `server.identifier` is empty, longer than 256 bytes, or contains whitespace or a control character THE BACKEND SHALL refuse to start with an error that names the field.
- CFG-003: WHEN `[auth]` is present without `enabled` THE BACKEND SHALL refuse to start with an error that names the field. An existing file that gained `[auth]` before this spec must state its intent, so auth cannot switch off silently.
- CFG-004: WHILE `auth.enabled` is `false` THE BACKEND SHALL serve every RPC without a credential check, SHALL NOT validate the other auth fields, and SHALL NOT load keys or fetch a JWKS.
- CFG-005: WHEN `server.min_libxmtp_version` is absent THE BACKEND SHALL publish an empty `min_libxmtp_version`, and any client version may connect.
- CFG-006: WHEN `server.min_libxmtp_version` is not a semantic version THE BACKEND SHALL refuse to start with an error that names the field.
- CFG-007: WHEN `limits.max_request_bytes` or `limits.max_response_bytes` is above 25 MiB THE BACKEND SHALL refuse to start with an error that names the field. 25 MiB is the fixed transport ceiling of spec 001 §11.
- CFG-008: WHEN the encoded `GetConfigurationResponse` is larger than 64 KiB THE BACKEND SHALL refuse to start with an error that names the response. This bounds the auth lists and the chain list without per-list limits.
- CFG-009: THE BACKEND SHALL publish the JSON schema for the new fields. The operator docs, the local test configuration, the Helm chart, and the four deploy guides (AWS ECS Fargate, Fly, Kubernetes Helm, Railway) SHALL include an example identifier with an instruction to change it.
- CFG-010: THE OPERATOR DOCS SHALL state that an identifier must never change after clients have connected, because every client database is bound to it (§6.3).

The backend does not validate `mls.max_group_members` against `limits.max_lookup_identifiers`. The client chunks lookups, so no relation between them is required.

### 4.2 Telemetry

- CFG-011: THE BACKEND SHALL add the identifier to the OTLP resource attributes under `xmtp.backend.identifier`, and SHALL reject a user-supplied resource attribute with that key the way it rejects `service.name`.
- CFG-012: THE BACKEND SHALL include the identifier as a field on every request completion log and every auth rejection log defined by ARC-095 and ARC-096.

## 5. Wire API

### 5.1 Service

`ConfigurationService.GetConfiguration(GetConfigurationRequest) returns (GetConfigurationResponse)` in package `xmtp.backend.v1`. The request is empty. It is the sixth service on the port, after the four application services and health.

- CFG-020: THE BACKEND SHALL serve `GetConfiguration` without a credential, on native gRPC and gRPC-Web, whatever `auth.enabled` is. The auth exemption becomes a list of two path prefixes: the gRPC health service and `ConfigurationService`. No other method joins it.
- CFG-021: THE BACKEND SHALL build the response once at startup from the validated configuration and SHALL return the same response for the life of the process.
- CFG-022: THE BACKEND SHALL report `ConfigurationService` in the health service, the telemetry route table, and the bounded service set of ARC-098, like every other service.
- CFG-023: WHERE a caller rate limit keyed on the IP address exists THE BACKEND SHALL apply it to `GetConfiguration`. WHERE a rate limit keyed on the JWT subject exists THE BACKEND SHALL NOT apply it, because the call carries no credential.
- CFG-024: THE BACKEND SHALL NOT include the JWKS URL, chain RPC URLs, public key material, leeway, JWKS refresh timing, or any database, telemetry, or listener setting in the response.

### 5.2 Response

Nested messages mirror the configuration file sections. Field numbers are assigned in the proto and never reused. Zero or empty means "not provided", and the client uses its compiled default, except where a row says otherwise.

| Message | Field | Wire type | Source | Client meaning of zero or empty |
| --- | --- | --- | --- | --- |
| `GetConfigurationResponse` | `identifier` | string | `server.identifier` | `ConfigurationInvalid`. |
| | `server_version` | string | backend crate version | Informational. |
| | `min_libxmtp_version` | string | `server.min_libxmtp_version` | No minimum. |
| | `auth` | `AuthConfiguration` | | Auth off. |
| | `retention` | `RetentionConfiguration` | | Compiled defaults. |
| | `limits` | `LimitsConfiguration` | | Compiled defaults per field. |
| | `mls` | `MlsConfiguration` | | Compiled defaults per field. |
| | `smart_contract_wallet_chains` | repeated string, CAIP-2 | keys of `[chains]` | Smart contract wallet signatures are disabled. |
| `AuthConfiguration` | `enabled` | bool | `auth.enabled` | Off. |
| | `keys` | repeated `SigningKey { kid string, alg string }` | inline keys, or the JWKS key set loaded at startup | No keys. |
| | `audiences`, `issuers`, `required_scopes` | repeated string | the same auth fields | Not required. |
| `RetentionConfiguration` | `group_message_seconds`, `welcome_seconds`, `key_package_seconds` | uint64 | `[retention]` | Compiled defaults. |
| `LimitsConfiguration` | `max_envelope_bytes`, `max_request_bytes`, `max_response_bytes` | uint64 | `[limits]` | Compiled default per field. |
| | `max_publish_topics`, `max_query_topics`, `max_query_limit`, `default_query_limit`, `max_newest_metadata_topics`, `max_newest_full_topics`, `max_update_adds`, `max_update_removes`, `max_stream_topics`, `max_static_topics`, `max_lookup_identifiers`, `max_scw_signatures`, `max_identity_entries`, `max_update_frames_per_second`, `max_update_burst`, `max_ping_frames_per_second`, `max_ping_burst` | uint32 | `[limits]` | Compiled default per field. |
| `MlsConfiguration` | `max_group_members`, `max_installations_per_inbox` | uint32 | `[mls]` | Compiled defaults. |
| | `commit_log_enabled` | optional bool | `[mls]` | Absent means compiled default. `false` is distinct from absent. |

The auth summary exists for operator tooling such as a future inspection CLI. The client acts only on `enabled` and `required_scopes`. `limits.max_http2_streams` is omitted as a transport detail. The three auth lists, the key list, and the chain list are bounded only by CFG-008.

- CFG-026: THE BACKEND SHALL fill every limit, retention, and MLS field from the validated configuration, so a client never sees zero for a value the backend enforces or publishes.
- CFG-027: WHILE `auth.enabled` is `true` AND `auth.jwks_url` is set THE BACKEND SHALL list the `kid` and `alg` of the key set loaded at startup and SHALL NOT update the list on a JWKS refresh.
- CFG-028: WHILE `auth.enabled` is `false` THE BACKEND SHALL send `AuthConfiguration` with `enabled` false and every other field empty.

## 6. Client behaviour

### 6.1 Configuration provider

The client owns a `ConfigProvider` abstraction in the shared configuration crate. A provider returns one immutable `ServerConfiguration` value from memory. Two providers exist: one backed by the stored copy, and one static provider that tests construct from any values. Every consumer listed in §6.4 reads through the provider. No consumer reads the database for a configuration value.

- CFG-030: THE CLIENT SHALL read the configuration once at build and SHALL hold that snapshot for the life of the client.
- CFG-031: WHEN a field is zero or empty in the stored copy THE CLIENT SHALL use the compiled default for that field, with the exceptions in the §5.2 table.
- CFG-032: THE CLIENT SHALL keep the compiled `BACKEND_DEFAULT_*` constants as the backend's defaults and as the client's fallbacks. The consumer-side constant for page size (`MAX_PAGE_SIZE`) has no consumer and SHALL be deleted.
- CFG-033: THE CLIENT BUILDER SHALL accept a provider. WHEN a provider is given THE CLIENT SHALL use it as the snapshot and SHALL NOT fetch, store, refresh, or check the identifier. This option exists for Rust tests and is not exposed through the bindings.

### 6.2 First connect, storage, and refresh

The stored copy lives in one new table with one row: the identifier, the backend URL the copy was fetched from, the serialized response, the fetch time, and an optional conflicting identifier. The table extends the single baseline migration. Databases created before this change are out of scope (§2).

- CFG-040: WHEN `build` runs online AND the database has no stored copy THE CLIENT SHALL call `GetConfiguration` before any identity work, validate the response (CFG-044), store it with the backend URL, and continue.
- CFG-041: WHEN the fetch in CFG-040 fails, or the store of its result fails, THE CLIENT SHALL fail `build` with the typed error `ConfigurationUnavailable` that carries the underlying error.
- CFG-042: WHEN `build` runs online AND the database has a stored copy AND the configured backend URL equals the stored URL THE CLIENT SHALL use the stored copy and SHALL NOT fetch, whatever client version wrote it. A changed URL is CFG-055. WHEN the stored response does not decode THE CLIENT SHALL log a warning, keep the stored identifier for CFG-051, and use compiled defaults for every value. A database with a stored identity always has a stored copy.
- CFG-043: WHEN `build` runs offline AND the database has no stored copy THE CLIENT SHALL use compiled defaults with an empty identifier and SHALL NOT fetch. WHEN `build` runs offline AND a stored copy exists THE CLIENT SHALL use it without the URL check of CFG-055; the check runs at the next online build.
- CFG-044: WHEN a response has an empty or invalid identifier (§4.1 rules), a `min_libxmtp_version` that does not parse, or a chain entry that is not a CAIP-2 identifier THE CLIENT SHALL fail with the typed error `ConfigurationInvalid` and SHALL NOT store it.
- CFG-045: THE CLIENT SHALL fetch `GetConfiguration` without a credential and without calling the auth callback. The client auth middleware exempts that method, and the static fetch of CFG-081 has no middleware.
- CFG-046: THE CLIENT SHALL run a refresh task as a worker under the existing worker controls. A run starts 3600 seconds plus up to 360 seconds of random jitter after the previous run ends, and the first run starts that long after build. A run makes up to 3 attempts, waiting 5 seconds then 30 seconds between attempts, each with the client's standard request timeout. After the third failure the run ends.
- CFG-047: WHEN a refresh attempt fails THE CLIENT SHALL log a warning for that attempt and SHALL keep the stored copy unchanged. There is no stale bound.
- CFG-048: WHEN a refresh succeeds AND no stored copy exists THE CLIENT SHALL store it. WHEN a refresh succeeds AND the identifier matches the stored one THE CLIENT SHALL replace the stored copy, the fetch time, and the URL. The snapshot is unchanged until the next build. WHEN the store fails THE CLIENT SHALL log a warning.
- CFG-049: THE CLIENT SHALL NOT trigger a refresh from any server error.
- CFG-050: Two processes on one database, such as an app and its notification extension, each run their own refresh. Both writes are valid because both come from the same backend.

### 6.3 Identifier binding

- CFG-051: WHEN a fetch at build (CFG-055), a refresh, or an explicit refresh returns an identifier that differs from the stored one THE CLIENT SHALL record the conflicting identifier in the stored row, log an error, fail that call with the typed error `BackendMismatch` that carries both identifiers, close every open stream with the same error, and fail every later call with it.
- CFG-052: WHEN `build` runs AND the stored row records a conflicting identifier THE CLIENT SHALL fail `build` with `BackendMismatch`.
- CFG-053: THE CLIENT SHALL NOT clear a recorded conflict. A refresh write never touches the conflict column, so a matching refresh cannot erase one. Recovery is a database created for the backend the app now uses.
- CFG-054: WHEN recording a conflict fails THE CLIENT SHALL keep the in-memory latch of CFG-051 for the life of the client. The next build then runs without the conflict and detects it again at its next fetch.
- CFG-055: WHEN `build` runs online AND the database has a stored copy AND the configured backend URL differs from the stored URL THE CLIENT SHALL fetch before any identity work. WHEN that fetch fails THE CLIENT SHALL fail `build` with `ConfigurationUnavailable`. WHEN the identifier differs from the stored one THE CLIENT SHALL apply CFG-051 and fail `build` with `BackendMismatch`. WHEN it matches THE CLIENT SHALL store the response with the new URL and continue. An operator may move a backend to a new URL; the identifier, not the URL, is the binding.

### 6.4 Applying values

- CFG-060: WHEN the snapshot `min_libxmtp_version` is higher than the client version, compared on major, minor, and patch, THE CLIENT SHALL fail `build` with the typed error `ClientVersionTooOld` that carries both versions.
- CFG-061: WHEN a refresh returns a `min_libxmtp_version` higher than the client version THE CLIENT SHALL store the copy, log an error, close every open stream with `ClientVersionTooOld`, and fail every later call with it. The client never advances a stream position because of it.
- CFG-062: WHEN `auth.enabled` is `true` AND no credential source was configured THE CLIENT SHALL fail `build` with the typed error `AuthRequired` that carries `required_scopes`. The transport reports whether a callback or handle was configured.
- CFG-063: WHEN `auth.enabled` is `false` AND the app provided an auth callback THE CLIENT SHALL CONTINUE TO call the callback and attach the credential. The backend ignores it.
- CFG-064: THE CLIENT SHALL chunk publishes at the snapshot `max_publish_topics` and `max_request_bytes`, chunk queries and metadata-only newest reads at `max_query_topics`, chunk full newest reads at `max_newest_full_topics`, chunk inbox-id lookups at `max_lookup_identifiers`, chunk smart contract wallet verifications at `max_scw_signatures`, cap query limits at `max_query_limit`, open one static subscription per `max_static_topics`, and cap stream update frames at `max_update_adds` and `max_update_removes`.
- CFG-065: WHEN an envelope is larger than the snapshot `max_envelope_bytes` THE CLIENT SHALL reject the publish with the existing too-large error before any network call. A value lowered on the backend after build is enforced by the backend with `INVALID_ARGUMENT` until the next build, and the client surfaces that rejection as it does today.
- CFG-066: WHEN the resolved inbox count of a group plus the additions would exceed the snapshot `max_group_members` THE CLIENT SHALL reject the change with the existing user-limit error before it builds the commit and before any publish.
- CFG-067: WHEN the installation count read from the inbox's identity log would exceed the snapshot `max_installations_per_inbox` THE CLIENT SHALL reject the registration with the existing installation-limit error before it publishes the identity update.
- CFG-068: WHILE `mls.commit_log_enabled` is `false` THE CLIENT SHALL NOT write commit-log entries and SHALL NOT read the commit log. The existing per-client commit-log worker switch still applies: the worker runs only when both allow it.
- CFG-069: WHEN the app asks the client to add or verify a smart contract wallet signature whose chain is not in the snapshot `smart_contract_wallet_chains` THE CLIENT SHALL reject it with the typed error `ChainNotAccepted` that carries the chain and the accepted list, before any network call. The check runs only where the app supplies a signature. It never runs during stream or catch-up processing, and it does not apply when the app supplied its own verifier.
- CFG-070: WHILE the snapshot `smart_contract_wallet_chains` is empty THE CLIENT SHALL reject every app-supplied smart contract wallet signature with `ChainNotAccepted`, under the same scoping as CFG-069.
- CFG-071: THE CLIENT SHALL keep the fixed 25 MiB transport ceiling on encoded requests and responses. CFG-007 guarantees the snapshot never exceeds it.

## 7. SDK surface

Every SDK exposes one typed `ServerConfiguration` object with every field in §5.2. Strings map to the language string. `bool` and `optional bool` map to the language boolean and its nullable form. `uint32` maps to the language's 32-bit or default integer. `uint64` maps to `Long` on Android, `UInt64` on iOS, and `number` in JavaScript, because every published value is below 2^53. Errors are distinct types, not message strings.

- CFG-080: THE SDKS SHALL expose `client.serverConfiguration()` returning the snapshot.
- CFG-081: THE SDKS SHALL expose a static `fetchServerConfiguration(url)` that calls the RPC with no database, no client, and no credential, so an app can learn `auth.enabled`, `required_scopes`, and the accepted chains before it builds a client.
- CFG-082: THE SDKS SHALL expose `client.refreshServerConfiguration()` that fetches now, applies CFG-044 and CFG-048 and CFG-051, and returns the fetched configuration. The running client's snapshot is unchanged.
- CFG-083: THE SDKS SHALL surface `ConfigurationUnavailable`, `ConfigurationInvalid`, `BackendMismatch`, `ClientVersionTooOld`, `AuthRequired`, and `ChainNotAccepted` as distinct error types.

## 8. Spec amendments

- Spec 001 §13 API-162 becomes: the backend serves the standard gRPC health service and `ConfigurationService`; both are unauthenticated. §14 drops the metadata-endpoint bullet and the review log records the reversal on 2026-09-15. §11 notes that the limits table is published to clients by spec 006, that request and response bytes are capped at 25 MiB by configuration validation, and that keepalive stays in `Started`.
- Spec 002 ARC-002 drops "No version or metadata endpoint is served" and "The gRPC port serves no additional endpoint", and names `ConfigurationService` as the sixth service on the port. ARC-098's bounded service set gains it. §8 configuration gains the fields in §4.1.
- Spec 004 is unchanged. STR-001 and STR-053 remain the only source of keepalive timing.

## 9. Recorded decisions for the future server-side version gate

Not implemented by this spec. Recorded from the owner's answers so the later project starts from them.

- The backend will reject a request whose `x-libxmtp-version` is below `server.min_libxmtp_version` with `FAILED_PRECONDITION`. The client must treat that status as retryable, halt all work, and never advance past an envelope because of it.
- The backend will reject a request with no version header.
- `GetConfiguration` and the health service stay exempt, so an old client can learn why it was rejected.
- `x-app-version` gets no minimum.

## 10. Alternatives considered

- Fetch on every build with the stored copy as a fallback. Rejected: the owner wants cache-first with an hourly refresh, and every build would pay a round trip. The URL check of CFG-055 covers the realistic case, a database reused against a different backend, without a round trip on every build.
- Return the identifier as a response header on every call. Rejected by the owner: identifiers almost never change, so per-call checking guards against nothing realistic.
- Push the configuration inside the stream `Started` frame. Rejected: wasm has no bidirectional stream, and the value is needed before identity work and before any stream opens.
- Mirror the full `[limits]` table with no curation. Rejected in favour of a curated set; `max_http2_streams` is a transport detail.
- Rely on the `[auth]` table for the enabled flag. Rejected: an explicit `enabled` field states intent, and CFG-003 stops an existing file from switching auth off by accident.
- Expose `auth.jwks_url`. Rejected: the URL validator accepts user info and query strings, so a URL can carry a secret. The key list covers the operator-tooling use.
- Store raw response bytes to keep unknown fields. Rejected: the transport decodes before the client sees bytes. A newer client rewrites the row at its first hourly refresh, so new fields arrive within an hour of an upgrade.
- Refetch at build when a different client version wrote the stored copy. Rejected by the owner: any stored copy will do, and a copy that does not decode falls back to defaults.
- Default the minimum client version to the backend's own version. Rejected by the owner as brittle: an unset minimum leaves the API open to every client version.

## 11. Libraries and utilities

External dependencies: none new. Semantic version parsing already exists in the client and `semver` is already a workspace dependency.

Internal modules: the shared configuration crate gains the provider trait and the `ServerConfiguration` type. The backend configuration module gains the new sections. The client transport gains one endpoint and one middleware exemption. The client database gains one table. The client worker framework gains one task.

## 12. Testing and validation

### 12.1 Regression protection

Preserved behaviours:

- CFG-090: THE BACKEND SHALL CONTINUE TO enforce every limit in spec 001 §11 at the configured value. Anchors: the wire-level limit tests in the client API crate and the backend service tests.
- CFG-091: THE BACKEND SHALL CONTINUE TO open every bidirectional stream with `Started(keepalive_interval_ms)`. Anchor: the keepalive assertion in the client limit tests.
- CFG-092: WHILE `auth.enabled` is `true` THE BACKEND SHALL CONTINUE TO apply every rule of the Phase 4.3 auth plan: key selection by `kid` and `alg`, issuer, audience, and scope checks, leeway, JWKS refresh and stale shutdown, rejection of an uncredentialed call on every service other than health and `ConfigurationService` with `UNAUTHENTICATED`, and secret-free diagnostics. Anchors: the backend auth layer and verifier tests.
- CFG-093: THE BACKEND SHALL CONTINUE TO exempt only the listed path prefixes. Anchor: the backend auth path-gate test, which changes to prove that exactly health and `ConfigurationService` are exempt.
- CFG-094: THE CLIENT SHALL CONTINUE TO build offline with `build_offline` and `allow_offline` without any network call, and SHALL CONTINUE TO run no worker, including the refresh task, when workers are disabled. Anchors: the offline builder tests and the worker control tests.
- CFG-095: THE CLIENT SHALL CONTINUE TO call the auth callback lazily on the first request that needs a credential, replay once after a rejection, and lock out after the configured consecutive failures. Anchors: the auth middleware tests.
- CFG-096: THE CLIENT SHALL CONTINUE TO keep a commit and its proposals in one publish chunk, measure encoded request size exactly, cap chunks in flight, keep response order, and re-chunk on a `TOO_LARGE` reason, as spec 001 API-142 requires. Anchors: the chunker tests in the client API crate.
- CFG-097: THE CLIENT SHALL CONTINUE TO treat a missing verifier route as retryable during ordered processing, as spec 004 STR-076 requires. `ChainNotAccepted` is raised only on app-supplied signatures (CFG-069). Anchors: the verifier tests.
- CFG-098: Every configuration file used by CI, the docs site, and the deploy guides SHALL be updated in this change, because CFG-001, CFG-003, and CFG-007 make them fail to start otherwise.

### 12.2 Test requirements

- CFG-100: THE TESTS SHALL cover every field that §6.4 acts on with a non-default value through the static provider and SHALL assert the behaviour changes with it. THE TESTS SHALL assert every other field of §5.2 round-trips to `serverConfiguration()` unchanged. `dev/backend/local.toml` keeps its current values.
- CFG-101: THE TESTS SHALL round-trip at least the identifier, one limit, `max_group_members`, `auth.enabled`, and the chain list through an ephemeral backend started from TOML.
- CFG-102: THE TESTS SHALL cover CFG-001, CFG-002, CFG-003, CFG-006, CFG-007, and CFG-008 with one failing configuration each, and CFG-004 and CFG-005 with one passing configuration each.
- CFG-103: THE TESTS SHALL cover CFG-040 to CFG-044, CFG-048, and CFG-051 to CFG-055 against an ephemeral backend, including a second backend with a different identifier reached by pointing the same database at its URL, and a URL change that keeps the same identifier.
- CFG-104: THE TESTS SHALL cover CFG-060 and CFG-061 with a client version one patch below the minimum, one equal to it, and one equal on major, minor, and patch with a different prerelease tag.
- CFG-105: THE TESTS SHALL cover CFG-069 and CFG-070 with a chain absent from the list and with an empty list, and SHALL prove a stream that carries an unknown-chain signature stays retryable.
- CFG-106: EACH SDK SHALL have one test that reads every field of `serverConfiguration()` and one that calls `fetchServerConfiguration(url)` against the shared backend.
- CFG-107: THE TESTS SHALL prove that `GetConfiguration` from a client with an auth callback never invokes the callback (CFG-045).

### 12.3 Edge cases

- Rolling deploy with two backends behind one load balancer that differ in a limit: the client holds one snapshot; a lowered value is enforced by the backend per CFG-065. A differing identifier across instances is an operator error and triggers CFG-051 at the next refresh that reaches the other instance.
- Refresh and build racing on one database: the stored row is one row, written whole; a reader sees either copy. The conflict column is written only by CFG-051 and never by a refresh.
- Backend restarted with a changed identifier: every client fails per CFG-051 at its next refresh. The docs forbid it (CFG-010).
- Same database reused with a different backend URL through an explicit database path: CFG-055 fetches at build and fails with `BackendMismatch` when the identifier differs.
- Older backend with no `ConfigurationService`: the fetch returns `UNIMPLEMENTED`, and build fails with `ConfigurationUnavailable`. There is no compatibility shim.
- App upgrade: the stored copy from the older client is used as is; fields it lacks read as compiled defaults until the first hourly refresh rewrites the row (CFG-042, CFG-048).
- Offline first build, then online: the snapshot holds defaults with an empty identifier; the first successful refresh stores the row (CFG-048) and the next build binds to it.
- Anyone who reaches the port can read the auth summary and chain list. SEC-005 already requires a trusted network. Key material and URLs are never included (CFG-024).
- Client clock is irrelevant: the fetch time is only for diagnostics.

### 12.4 Verification commands

```bash
dev/nix-shell 'just lint'
dev/nix-shell 'just lint-proto'
dev/nix-shell 'just backend test'
dev/nix-shell 'just test'
dev/nix-shell 'just validation'
dev/check-ephemeral-backend
dev/nix-shell 'just docs lint'
```

## Review log

| Date | Change |
| --- | --- |
| 2026-09-15 | Draft from the answered question list. |
| 2026-09-15 | Revision 2 after a Codex adversarial review (7 critical, 20 major). Added the `x-xmtp-backend-id` response header and live mismatch latch; removed `jwks_url` from the response; auth off skips key loading; backend caps request and response bytes at 25 MiB and the response at 64 KiB; wire types per field; refresh schedule with jitter and fixed retry delays; refresh exempt from the auth callback; version latch on refresh; writer version on the stored row; provider injection through the builder; group and installation checks reworded around the remote state they need; chain check scoped to app-supplied signatures; regression requirements for chunking, auth rules, worker controls, and verifier retryability; test lists corrected. |
| 2026-09-15 | Revision 3 from the owner's review comments: no default minimum client version, an unset field leaves the API open; any stored copy is used at build whatever client wrote it, with compiled defaults when it does not decode; the writer-version column and its refetch rule are gone. |
| 2026-09-15 | Revision 4 from the owner's review comments: the per-call `x-xmtp-backend-id` response header is dropped; the stored row records the backend URL and a changed URL fetches again at build (CFG-055); prerelease tags are ignored in the version compare. No clarification markers remain. |
