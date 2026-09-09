---
title: Reactions
---

A reaction is a short response to an earlier message.

Type ID: `xmtp.org/reaction:2.0`. The payload is a `Reaction` protobuf. The fallback describes the reaction and the earlier message. The Rust codec sets `shouldPush` to `false`. Kotlin and Swift set it to `true` when the action is `Added`.

| Field              | Meaning                                      |
| ------------------ | -------------------------------------------- |
| `reference`        | ID of the message that receives the reaction |
| `referenceInboxId` | Inbox ID of the referenced message sender    |
| `action`           | Add or remove the reaction                   |
| `schema`           | Unicode, short code, or custom schema        |
| `content`          | The reaction value                           |

Use the schema that matches the content. A Unicode emoji uses the Unicode schema. The Agent SDK provides `ctx.sendReaction()`.
