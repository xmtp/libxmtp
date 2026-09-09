---
title: Replies
---

A reply points to an earlier message and contains another encoded content value.

Type ID: `xmtp.org/reply:1.0`. The payload contains a nested `EncodedContent`, a message reference, and the referenced sender inbox ID. A text reply fallback is `Replied with "…" to an earlier message`. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Field              | Meaning                                |
| ------------------ | -------------------------------------- |
| `reference`        | ID of the earlier message              |
| `referenceInboxId` | Inbox ID of the earlier message sender |
| `content`          | Nested encoded content                 |

Decode the nested content recursively with the codec registry. Do not assume that every reply contains text. In the Agent SDK, `EnrichedReply.inReplyTo` contains the referenced message when it is available.
