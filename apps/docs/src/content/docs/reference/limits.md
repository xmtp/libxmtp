---
title: Rate limits and capacity
---

The backend limits below show the default configuration values. Client limits are listed separately.

## Rate limits

The backend does not apply a per-caller rate limit. Each bidirectional stream has separate token buckets for update frames and client ping frames: 10 frames per second with a burst of 100. Pong frames do not consume either bucket. Exhaustion closes the stream with `RESOURCE_EXHAUSTED`. Reconnect with backoff from durable per-topic cursors.

## Backend request limits

A request above a structural limit is rejected with `INVALID_ARGUMENT`. An
oversized publish envelope is also rejected with `INVALID_ARGUMENT`. A
message-size rejection from the transport arrives as `OUT_OF_RANGE`. Reduce
the batch or payload before sending it again. Query row limits are clamped to
the configured maximum.

| Limit                                                  |                       Default |
| ------------------------------------------------------ | ----------------------------: |
| Envelope bytes                                         |                         1 MiB |
| Request bytes                                          |                        25 MiB |
| Response bytes                                         |                        25 MiB |
| Distinct topics in one publish                         |                         1,000 |
| Topics in one query                                    |                         1,000 |
| Rows returned by one query                             | 100 by default, 1,000 maximum |
| Newest-envelope topics, metadata only                  |                         1,000 |
| Newest-envelope topics, full envelopes                 |                           100 |
| Identifiers in one inbox-ID lookup                     |                           250 |
| Signatures in one smart-contract-wallet verify request |                           100 |
| Identity-update entries per inbox                      |                           256 |

## Backend stream limits

| Limit                                         |                  Default |
| --------------------------------------------- | -----------------------: |
| Topics registered on one bidirectional stream |                  100,000 |
| Topics added by one stream update             |                  100,000 |
| Topics removed by one stream update           |                  100,000 |
| Topics in one static subscription             |                   10,000 |
| Stream update frames                          | 10 per second, burst 100 |
| Client ping frames                            | 10 per second, burst 100 |
| Keepalive interval                            |                     30 s |
| Concurrent HTTP/2 streams on one connection   |                      100 |

A stream that exhausts its token bucket receives `RESOURCE_EXHAUSTED`. Reconnect with backoff from your durable per-topic cursors.

The server advertises the HTTP/2 stream limit through the transport. A client
that follows that limit queues new requests until an existing stream closes.
Long-lived subscriptions also consume HTTP/2 streams. This is not a per-caller
request quota.

## Client limits

The first two values are defaults published in the backend's `[mls]`
configuration. An operator can change them, and clients apply the configured
values locally. The metadata byte limits are fixed in the client library.

| Limit                          | Value |
| ------------------------------ | ----: |
| Members in one group           |   250 |
| Active installations per inbox |    10 |
| Group `appData` bytes          | 8,192 |
| Group name bytes               |   100 |
| Group description bytes        | 1,000 |
| Group image URL bytes          | 2,048 |

A group message is bounded by the backend's envelope limit, which defaults to
1 MiB. The payload must be smaller because the encoded envelope also contains
MLS ciphertext and framing data.

The group cap counts inboxes, not installations. The installation cap applies separately to each inbox. See [Manage inboxes](/sdk/inboxes/).
