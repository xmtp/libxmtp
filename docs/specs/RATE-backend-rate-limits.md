---
prefix: RATE
status: draft
---
# Backend rate limits

The backend charges every request to a caller and refuses work a caller has no allowance for. Each request costs units in one or more categories of work, such as envelopes published or topics queried, and the backend spends that cost against the caller's limits for the category before it does the work. A spend that would exceed a limit is refused with the delay after which the same spend can be admitted. Without this, one caller can make the backend publish, read, verify chain signatures, and hold stream state without bound, and every other caller pays for it.

## Scope

In scope: which caller a request is charged to; the categories of work and the cost of each request in each; what a limit admits; which limits apply to a caller; how a stream pays for what it holds and reads; the rejection a caller receives and what a client does with it; the memory the limiter may use.

Out of scope: credentials and admission (AUTH); the configuration keys, their types, and their checks (CONF section 1.2); the status code table (API section 7); the per-stream frame buckets (API-258); request bounds (API section 7); the metric families (OPS section 5).

| Related | Relation |
| --- | --- |
| [AUTH](AUTH-backend-auth.md) | Admits the request and names the API key or JWT claims this spec forms a caller key from. A request auth rejects is never charged. |
| [API section 7](API-backend-api.md#7-bounds-errors-and-transport) | Owns the status code of a rate-limit rejection. This spec owns its details and what a client does with them. |
| [CONF section 1.2](CONF-backend-configuration.md#12-keys-and-checks) | Owns the `rate_limits` keys and their checks. This spec owns what the keys mean and the default limits. |
| [OPS section 5](OPS-backend-operations.md#5-telemetry) | Owns the rate-limit metric families. OPS-014 keeps `X-Forwarded-For` values and any client address computed from them out of telemetry, and AUTH-027 keeps JWT claim values out. |
| [PUSH section 8](PUSH-push-subscriptions.md#8-the-client) | PUSH-267 keeps notifications enabled after a rate-limit rejection. |

## Terms

| Term | Meaning |
| --- | --- |
| Caller key | The value a request's charges are recorded under, formed under RATE-001. |
| Auth type | `api_key`, `jwt`, or `ip`: the form of a caller key. |
| Client address | The IP address RATE-002 selects for a request. |
| Trusted proxy | An address inside a network listed in `rate_limits.trusted_proxies`, which lists none by default. |
| Category | A named kind of work with its own unit and limits, listed in the category table. |
| Cost | The number of units of one category a step of a request charges. |
| Limit | A `count` of units and a `window_seconds`. |
| Spend | One charge of one cost in one category to one caller key, admitted or denied as a whole. |
| Retry delay | The time RATE-024, or RATE-032 for a rent denial and RATE-034 for a catch-up-target denial, reports with a denied spend. |
| Earned back | An admitted spend of cost n at time t under a limit with `count` C and `window_seconds` W is earned back at max(t, E′) + n × W / C, rounded down to a whole nanosecond, where E′ is when the spend admitted before it under that limit for the same caller key was earned back. |
| Stream rent | The `stream_rent` cost a stream prepays for each 60-second period it stays open. |
| Catch-up read | A read of the store a stream makes to deliver the envelopes of its registrations above their cursors. The read of an `Update`'s catch-up target is not one. |

## 1. Callers

A request is charged to the most specific identity the backend holds for it. An API key names an operator's own caller. A JWT names a user of an identity provider by its issuer and subject; two issuers can issue the same subject, so the issuer is part of the key. A request with neither is charged to the address it came from.

A backend runs behind a proxy, which terminates TLS and connects from its own address. The proxy appends the address it received the request from to `X-Forwarded-For`, and every hop before it may have done the same, or the caller may have written anything at all. Only the entries written by proxies the operator trusts are evidence, so the backend reads the list from the right, skips the trusted proxies, and stops at the first address it does not trust. An IPv6 subscriber typically holds a whole /64, so the backend treats the /64 as one caller.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-001 | Caller key by credential | When auth admits a request under an API key (AUTH-006), the backend MUST charge it to the auth type `api_key` with the key's name; when auth admits a JWT whose claims carry `sub`, to the auth type `jwt` with the token's `iss`, or the empty string when it has none, and its `sub`; otherwise to the auth type `ip` with the client address. Two caller keys with different auth types MUST NOT share a charge. | Keyed on `sub` alone, a user of one trusted issuer spends the allowance of a user of another; an API key named like an address would share that address's charges. |
| RATE-002 | Client address from trusted proxies | When the peer address of a request's connection is a trusted proxy and the request carries one or more `X-Forwarded-For` headers, the backend MUST read their comma-separated entries in header order as one list and select, scanning from the right, the first entry that is an IPv4 or IPv6 address and not a trusted proxy, or the leftmost entry when every entry is a trusted proxy; when an entry it scans before it selects one is not an IPv4 or IPv6 address, or when the peer is not a trusted proxy or no such header is present, the backend MUST select the peer address. The backend MUST compare an IPv4-mapped IPv6 address with `rate_limits.trusted_proxies` as its IPv4 address. | A header from a peer the operator does not trust is whatever the caller wrote, so a caller could charge every request to a fresh address or to a victim's; an IPv4 proxy seen on a dual-stack listener would otherwise never match its network. |
| RATE-003 | IPv6 callers by /64 | When the client address is an IPv4-mapped IPv6 address, the backend MUST form the caller key from the IPv4 address, and when it is any other IPv6 address, from its first 64 bits. | One subscriber holds a whole /64, and a key per address would give that subscriber 2^64 fresh allowances. |

## 2. Categories and costs

The category table is the whole list of charges. A request charges only the categories whose rows name it; a health request, a `GetConfiguration` request, and a request to a path no service implements charge nothing. A cost is counted in the request as the backend received it, before duplicates or repeated topics are merged. A step is charged before it runs, so a denied step does no work, and a step an earlier denial prevented is never charged.

Every charged RPC spends 1 `request` unit, and a denial of that spend decides the request's status over every condition of API section 7, PUSH-264, and the PUSH and ATCH admission tables (RATE-014). Every other charge is spent by the step that does the work it pays for, before that work (RATE-010). Each category's cost for one step is spent whole, in one spend. A request refused after a spend keeps that spend and is charged nothing more.

A `request` unit pays for one RPC, for one read of the store a stream makes to catch up, or for the read of the catch-up target of an `Update` that newly registers topics. Reading one topic and reading the most topics the request bounds of API section 7 allow cost the same unit, because both are one read of the store and those bounds cap the work of one read.

| Category | Unit | Charged by | Cost |
| --- | --- | --- | --- |
| `request` | request or store read | every charged RPC; each read of the store a stream makes to catch up its registrations; each `Update` that newly registers a topic, for the read of its catch-up target | 1 |
| `publish` | envelope | `Publish` | The number of entries in `envelopes`, or 1 when there are none |
| `identity_update` | identity update | `Publish` | The number of entries in `envelopes` whose payload is an `IdentityUpdate` |
| `chain_verify` | signature | `Publish`; `VerifySmartContractWalletSignatures` | For a publish, the number of ERC-6492 signatures across its identity updates; for a verify request, the number of request entries, or 1 when there are none |
| `push` | topic | `UpdateSubscriptions`, at check 2 of the PUSH admission table | The number of entries in `adds` |
| `upload` | upload | `CreateUpload`, at the row of the ATCH admission table that names it | 1 |
| `stream_rent` | rent unit per 60 s | an open stream | 1 + ceil(registered topics / 1000) for each period, under RATE-030 and RATE-031 |

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-010 | Charge before work | When a request reaches a step the category table charges, the backend MUST spend that step's cost for the request's caller key before it reads the store, calls a chain, stores an envelope, registers a topic, stores a subscription, or obtains a storage credential for that step, and when the spend is denied MUST NOT perform the step. | A charge after the work lets a caller over its limit still make the backend do the work. |
| RATE-011 | Only the category table charges | The backend MUST NOT charge a request a category the category table does not give it, or more units of a category than the table gives it, and MUST NOT charge a health request, a `GetConfiguration` request, a request to a path no service implements, or a request that auth rejects. | An operator sizes a limit from the table; a charge it does not list denies callers for work they did not do, and a charged `GetConfiguration` locks a client out of learning the deployment's requirements. |
| RATE-012 | Exempt while disabled | While `rate_limits.enabled` is `false`, the backend MUST NOT deny any request or stream under this spec. | |
| RATE-014 | Request charge first | When the `request` spend of a charged RPC is denied, the backend MUST fail the request with the rejection RATE-040 gives, whatever condition of the API status table, a bound of API section 7 or PUSH-264, or an admission table of PUSH or ATCH the request also meets. | A rule that let a later check decide the code would make the backend decode and validate a request it refuses for its caller's allowance. |

## 3. Limits

A limit admits a sustained rate of `count` units per `window_seconds`, with a burst of up to `count` units. A caller that has spent nothing for a whole window can spend its whole count at once, and over any span of time a caller is admitted at most one count plus the units the rate earns in that span, except that one spend larger than a count is admitted whole when every earlier spend has been earned back. A spend larger than a limit's count is admitted only when every earlier spend under that limit has been earned back, and the caller then waits until the rate has earned back the whole cost, so one large request cannot do more work than the rate allows. A category can carry up to three limits on different windows, for example a minute and an hour; a spend must fit every one of them, and a spend that does not fit one charges none of them.

The limits that apply come from the most specific configuration that names the caller: an override for its caller key, else the limits for its auth type, else the category's `default`, else the default limits table below. An empty list of limits admits every spend.

| Category | Default limits |
| --- | --- |
| `request` | 6000 per 60 s; 200000 per 3600 s |
| `publish` | 1000 per 60 s; 20000 per 3600 s |
| `identity_update` | 10 per 60 s; 100 per 3600 s |
| `chain_verify` | 200 per 60 s; 2000 per 3600 s |
| `push` | 2000 per 60 s; 20000 per 3600 s |
| `upload` | 120 per 60 s; 2000 per 3600 s |
| `stream_rent` | 120 per 60 s |

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-020 | Admission rule | On one instance, while it keeps a caller key's state, for each limit with `count` C and `window_seconds` W that applies to the caller key in a category, let E be the time every spend the instance admitted under that limit for the caller key is earned back, or the spend's own time t when that is later; the backend MUST admit a spend of cost n > 0 when, for every such limit, E + n × W / C − t is not greater than W, or n is greater than C and E equals t, and MUST deny it otherwise. A spend of cost 0 MUST be admitted with no charge. | A rule that admits more does not bound the work the limit was set to bound; one that admits less refuses callers within their limits, or refuses a request larger than a count for ever. |
| RATE-022 | Denial charges nothing | When a spend is denied, the backend MUST NOT charge any limit of that category for it. | A denied request that still spends allowance lets a client that retries too early lock itself out for ever. |
| RATE-024 | Exact retry delay | When a spend is denied, other than a `stream_rent` spend whose delay RATE-032 sets or a `request` spend whose delay RATE-034 sets, the backend MUST report as retry delay the whole number of milliseconds, rounded up, after which the same spend is admitted on that instance if the caller key spends nothing more in that category there and the instance keeps its state until then; the same spend made 1 ms before that delay MUST be denied. | A delay that is too short brings the retry back to be denied; one that is too long holds a client off longer than its limit requires. |
| RATE-025 | Most specific limits apply | When `rate_limits.overrides` holds an entry for the request's caller key that names the category, the backend MUST apply exactly that entry's limits for the category; otherwise, when the category's configuration names the caller key's auth type, exactly those limits; otherwise, the category's `default`, or the default limits table when the category sets no `default`. An empty list MUST admit every spend in that category. | An operator who raises a bot's limits must not find a general limit still capping it. |

## 4. Streams

A stream costs the backend memory for every topic it holds, for as long as it stays open, and a read of the store for every batch of history it catches up on. Delivering a live envelope costs almost nothing: the backend reads each new envelope once and hands it to every stream that holds its topic. A stream therefore pays rent for what it holds, a `request` unit for each catch-up read, and a `request` unit for the catch-up target of each `Update` that newly registers a topic, and live delivery is free.

Rent is prepaid for each 60-second period, starting at open, so it also bounds how many streams a caller can open and keep open. An `Update` that raises the rent within a period pays the difference before its topics are registered; one that lowers it is not refunded. When any rent spend is denied, the backend ends the stream, because ending it is the only way to release what it holds; the client reconnects from its cursors, so nothing is skipped (API-254), and the rejection's retry delay covers the whole rent the replacement stream pays for those topics, its open included (RATE-032). When a catch-up read is denied, the backend reads nothing for those registrations until the retry delay has passed, and keeps the stream open. When the `request` spend for an `Update`'s catch-up target is denied, the backend ends the stream, with a delay that covers the replacement stream's `Subscribe` and that read (RATE-034). That unit is spent before any topic of the update is registered, and the target is read after registration (API-252). An `Update` whose rent or catch-up-target spend is denied is not accepted: the stream ends before its `Applied`.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-030 | Rent is prepaid | When a stream opens, and when each 60-second period after its open begins, the backend MUST spend `stream_rent` of 1 + ceil(t / 1000) for that period, where t is the number of topics registered on the stream at that moment, or the number in `topics` for a `SubscribeStatic` open. | A stream that pays only to open holds topics for hours at the price of a minute. |
| RATE-031 | Updates pay rent before registering | When an `Update` would leave t' topics registered on a stream and 1 + ceil(t' / 1000) is greater than the `stream_rent` already spent for the current period, the backend MUST spend `stream_rent` equal to the difference before it registers any topic of the update or sends its `Applied`. | An update that registers topics before paying for them holds memory the caller has no allowance for. |
| RATE-032 | Rent denial ends the stream | When a `stream_rent` spend under RATE-030 or RATE-031 is denied for an open stream, the backend MUST end the stream with the rejection RATE-040 gives and MUST NOT register any topic of the update whose spend was denied. The retry delay of that rejection MUST be the RATE-024 delay for one spend of 1 + ceil(t' / 1000), where t' is the number of topics the stream would hold had the denied spend been admitted. | Paused delivery keeps every registration in memory; only ending the stream releases it. A delay that covers only the denied spend is short by the open charge of the replacement stream, so the reopened stream is denied again on every retry. |
| RATE-033 | Read denials delay, never skip | When the `request` spend for a catch-up read is denied, the backend MUST NOT read or deliver envelopes for the registrations that read covers before the retry delay has passed, MUST keep the stream open, and MUST retry the spend no earlier than the retry delay. | A caller over its read allowance would otherwise lose its stream, and reconnecting costs more reads than waiting. |
| RATE-034 | Target-read denial ends the stream | When the `request` spend for the read of an `Update`'s catch-up target is denied, the backend MUST end the stream with the rejection RATE-040 gives, MUST NOT register any topic of that update or send its `Applied`, and MUST report as retry delay the RATE-024 delay for one spend of 2 `request` units. | A delay that covers only the denied spend is short by the `request` unit of the replacement stream's `Subscribe`, so the reopened stream's `Update` is denied again on every retry. |

## 5. Rejection

A rate-limit rejection says that the request was refused for its caller's allowance, in which category, and how long to wait. It uses the standard gRPC error details, so any gRPC client library can read the delay. API section 7 gives the status code.

| Detail | Field | Value |
| --- | --- | --- |
| [`google.rpc.ErrorInfo`](https://github.com/googleapis/googleapis/blob/master/google/rpc/error_details.proto) | `reason` | `RATE_LIMITED` |
| | `domain` | `xmtp.org` |
| | `metadata` | one entry, `category`, whose value is the category name from the category table |
| [`google.rpc.RetryInfo`](https://github.com/googleapis/googleapis/blob/master/google/rpc/error_details.proto) | `retry_delay` | the retry delay of RATE-024, or of RATE-032 for a `stream_rent` denial and of RATE-034 for a catch-up-target denial |

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-040 | Rejection details | When the backend fails a request or a stream because a spend was denied, it MUST attach to the status the `google.rpc.ErrorInfo` and `google.rpc.RetryInfo` details the rejection table gives, and MUST NOT put the caller key, the client address, or any credential value in the status message. | A client that cannot tell a rate limit from an oversized request splits the request and multiplies the load (CONF-073). |
| RATE-041 | Client waits the retry delay | When a request or a stream fails with an `ErrorInfo` whose `reason` is `RATE_LIMITED` and whose `domain` is `xmtp.org`, the client MUST NOT resend that request through its own retry of the refused call, or open a stream to replace that stream, before the `RetryInfo` delay has passed, and MUST NOT split that request into smaller requests because of the rejection. When such a rejection carries no `RetryInfo`, or a negative `retry_delay`, the first sentence sets no minimum wait. | A retry or a reopened stream inside the delay is denied again and spends the caller's allowance on nothing. |

## 6. Memory

The limiter's memory is fixed by `rate_limits.max_memory_mib` and does not grow with the number of callers. A caller back at full allowance holds no state. A caller key can be recorded in only a small part of the state. When that part has no room, the backend forgets a caller recorded there, rather than refuse a request, so a flood of new callers costs other callers some of their recorded spend, never their admission.

| ID | Title | Requirement | Why |
| --- | --- | --- | --- |
| RATE-050 | Memory bound | The backend MUST NOT hold more than `rate_limits.max_memory_mib` MiB of rate-limit state, whatever the number of caller keys. | A flood of fresh addresses would otherwise grow the state until the instance is killed. |
| RATE-051 | Full state never denies | When the backend has no room to record a caller key's state where that key can be recorded, it MUST NOT deny a spend because of that, and MUST forget instead the state, among those it could replace, whose admitted spends are all earned back soonest. | Refusing new callers when full lets a flood of fresh keys deny service to every caller it did not already know. |
| RATE-052 | State is kept until earned back | The backend MUST keep a caller key's state for a limit until every spend the instance admitted under that limit for that key is earned back, unless it forgets that state under RATE-051 or the instance restarts. | A limiter that forgets state early admits more than the limit, and no other row refuses it. |

## Known limitations

Limits apply per instance. A deployment that runs N instances behind a balancer admits up to N times each limit to a caller whose requests are spread across them.

An instance forgets every caller's spend when it restarts. It also forgets a caller's spend under RATE-051, which can start long before the whole state is full, because a caller key can be recorded in only a small part of it. The caller forgotten is the one whose spends are earned back soonest, so a heavy spender is forgotten last. A forgotten caller receives a fresh allowance. A caller that floods fresh caller keys gains more from the fresh allowances of those keys than from what its flood makes the instance forget.

Callers behind one IPv4 address, such as the subscribers of a carrier-grade NAT, share one caller key and one allowance when they hold no credential.

Behind a proxy that `rate_limits.trusted_proxies` does not list, every caller without a credential is charged to the proxy's address and shares one allowance. A listed proxy that passes on a client's `X-Forwarded-For` without appending the address it received the request from lets that client choose the address it is charged to.

A request that auth rejects is not charged, so a flood of requests with bad credentials is limited only by the cost of rejecting them.

A step that is denied after an earlier step of the same request was admitted keeps the earlier charge.

RATE-041 holds the client's own retries of a refused request, not a request an app's next action sends: a sync resends a publish that failed, an upload the app retries begins with `CreateUpload` (ATCH section 4), and an enable sends `Register` (PUSH-266), any of them before the retry delay has passed. Such a request is denied again, and keeps the charge of each of its steps admitted before the denied one, its `request` unit included when that spend was not the one denied.
