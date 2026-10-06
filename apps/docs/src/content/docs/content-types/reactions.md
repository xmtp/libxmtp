---
title: Reactions
---

A reaction is a short response to an earlier message.

Type ID: `xmtp.org/reaction:2.0`. The payload is a `ReactionV2` protobuf. The fallback describes the reaction and the earlier message. `shouldPush` defaults to `false` on Browser, Node, Kotlin, and Swift.

| Field              | Meaning                                            |
| ------------------ | -------------------------------------------------- |
| `reference`        | ID of the message that receives the reaction       |
| `referenceInboxId` | Optional inbox ID of the referenced message sender |
| `action`           | Add or remove the reaction                         |
| `schema`           | Unicode, short code, or custom schema              |
| `content`          | The reaction value                                 |

The SDK codec takes a message reference, an optional sender inbox ID, and a nested `reaction` value with `action`, `schema`, and `content`. The wire protobuf stores these fields together. Its action codes are `0` (unspecified), `1` (added), and `2` (removed). Its schema codes are `0` (unspecified), `1` (Unicode), `2` (shortcode), and `3` (custom).

Use the schema that matches the content. A Unicode emoji uses the Unicode schema. The Agent SDK provides `ctx.sendReaction()`.
