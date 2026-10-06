---
title: Replies
---

A reply points to an earlier message and contains another encoded content value.

Type ID: `xmtp.org/reply:1.0`. The payload bytes contain a nested `EncodedContent`. The `reference` parameter names the earlier message. The optional `referenceInboxId` parameter names its sender. The `contentType` parameter contains the nested content type ID. A text reply fallback is `Replied with "…" to an earlier message`. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Field              | Meaning                                         |
| ------------------ | ----------------------------------------------- |
| `reference`        | ID of the earlier message                       |
| `referenceInboxId` | Optional inbox ID of the earlier message sender |
| `content`          | Nested encoded content                          |

The SDK decodes the nested content as a message body. Do not assume that every reply contains text. `message.replyContent` contains this decoded body. `message.inReplyTo` contains the parent summary when it is available. Use `message.parent()` to load the parent message. Replies with non-text content use the fallback `Replied to an earlier message`.
