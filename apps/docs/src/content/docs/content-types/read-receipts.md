---
title: Read receipts
---

A read receipt records that a participant read the conversation. Its timestamp is the message envelope timestamp.

Type ID: `xmtp.org/readReceipt:1.0`. The payload is empty. It has no fallback. `shouldPush` defaults to `false` on Browser, Node, Kotlin, and Swift.

Read receipts are background events. Filter them from normal message lists. A client that does not support the type receives no decoded content and no fallback, so it must drop the message.
