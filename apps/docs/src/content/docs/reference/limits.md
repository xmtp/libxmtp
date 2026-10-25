---
title: Rate limits and capacity
---

The backend limits below show each configuration key and its default. Client
limits are listed separately. Each count and rate limit under `[limits]` can be
set from 1 to 4294967295, except `limits.max_push_topics`, which stops at 2147483647. The
[configuration field table](/specs/conf-backend-configuration/#12-keys-and-checks)
gives the range and checks of every key.

## Rate limits

The backend does not apply a per-caller rate limit. Each bidirectional stream has separate token buckets for update frames and client ping frames: 10 frames per second with a burst of 100. Pong frames do not consume either bucket. Exhaustion closes the stream with `RESOURCE_EXHAUSTED`. Reconnect with backoff from durable per-topic cursors.

## Backend request limits

A request above a structural limit is rejected with `INVALID_ARGUMENT`. An
oversized publish envelope is also rejected with `INVALID_ARGUMENT`. A
message-size rejection from the transport arrives as `OUT_OF_RANGE`. Reduce
the batch or payload before sending it again. Query row limits are clamped to
the configured maximum.

| Limit                                                  | Key                                 | Default |
| ------------------------------------------------------ | ----------------------------------- | ------: |
| Envelope bytes                                         | `limits.max_envelope_bytes`         |   1 MiB |
| Request bytes                                          | `limits.max_request_bytes`          |  25 MiB |
| Response bytes                                         | `limits.max_response_bytes`         |  25 MiB |
| Distinct topics in one publish                         | `limits.max_publish_topics`         |   1,000 |
| Topics in one query                                    | `limits.max_query_topics`           |   1,000 |
| Rows returned by a query that sets no limit            | `limits.default_query_limit`        |     100 |
| Rows returned by one query                             | `limits.max_query_limit`            |   1,000 |
| Newest-envelope topics, metadata only                  | `limits.max_newest_metadata_topics` |   1,000 |
| Newest-envelope topics, full envelopes                 | `limits.max_newest_full_topics`     |     100 |
| Identifiers in one inbox-ID lookup                     | `limits.max_lookup_identifiers`     |     250 |
| Signatures in one smart-contract-wallet verify request | `limits.max_scw_signatures`         |     100 |
| Bytes in one smart-contract-wallet signature           | fixed                               |  16,384 |
| Identity-update entries per inbox                      | `limits.max_identity_entries`       |     256 |

Request and response bytes cannot exceed the fixed 25 MiB transport ceiling.

## Backend stream limits

| Limit                                         | Key                                                              |                  Default |
| --------------------------------------------- | ---------------------------------------------------------------- | -----------------------: |
| Topics registered on one bidirectional stream | `limits.max_stream_topics`                                       |                  100,000 |
| Topics added by one stream update             | `limits.max_update_adds`                                         |                  100,000 |
| Topics removed by one stream update           | `limits.max_update_removes`                                      |                  100,000 |
| Topics in one static subscription             | `limits.max_static_topics`                                       |                   10,000 |
| Stream update frames                          | `limits.max_update_frames_per_second`, `limits.max_update_burst` | 10 per second, burst 100 |
| Client ping frames                            | `limits.max_ping_frames_per_second`, `limits.max_ping_burst`     | 10 per second, burst 100 |
| Keepalive interval                            | `streams.keepalive_interval_ms`                                  |                     30 s |
| Concurrent HTTP/2 streams on one connection   | `limits.max_http2_streams`                                       |                      100 |

A stream that exhausts its token bucket receives `RESOURCE_EXHAUSTED`. Reconnect with backoff from your durable per-topic cursors.

The server advertises the HTTP/2 stream limit through the transport. A client
that follows that limit queues new requests until an existing stream closes.
Long-lived subscriptions also consume HTTP/2 streams. This is not a per-caller
request quota.

## Backend push limits

| Limit                                | Key                      | Default |
| ------------------------------------ | ------------------------ | ------: |
| Topics stored for one push recipient | `limits.max_push_topics` | 100,000 |

See [Push configuration](/deploy/push-configuration/) for the other push
settings.

## Handle limits

The minimum name length is the backend's `handles.min_name_length`. An
operator can change it from 1 to 27; a change applies only to later claims.

| Limit                                               | Key                       | Default |
| --------------------------------------------------- | ------------------------- | ------: |
| Shortest name part, the text before `#`, in a claim | `handles.min_name_length` |       3 |
| Longest name part                                   | fixed                     |      27 |
| Handle claims per inbox                             | fixed                     |      64 |
| Days between two claims by one inbox                | fixed                     |       7 |

Each claim uses one identity-update entry of the inbox. At the default of 256
entries per inbox, 192 entries stay for membership changes.

## Client limits

The first two values are defaults published in the backend's `[mls]`
configuration. An operator can change them, and clients apply the configured
values locally. `mls.max_installations_per_inbox` can be set from 1 to 100, the
number of actions one identity update can carry, so one update can revoke every
installation an inbox may hold. The other limits are fixed in the client
library.

| Limit                          | Key                               | Value |
| ------------------------------ | --------------------------------- | ----: |
| Members in one group           | `mls.max_group_members`           |   250 |
| Active installations per inbox | `mls.max_installations_per_inbox` |    10 |
| Actions in one identity update | fixed                             |   100 |
| Group `appData` bytes          | fixed                             | 8,192 |
| Group name bytes               | fixed                             |   100 |
| Group description bytes        | fixed                             | 1,000 |
| Group image URL bytes          | fixed                             | 2,048 |

A group message is bounded by the backend's envelope limit, which defaults to
1 MiB. The payload must be smaller because the encoded envelope also contains
MLS ciphertext and framing data.

The group cap counts inboxes, not installations. The installation cap applies separately to each inbox. A call that revokes more than 100 installations or identities fails with an invalid-input error before it asks for a signature; split it into several calls. See [Manage inboxes](/sdk/inboxes/).
