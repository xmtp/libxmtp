---
title: Read receipts
---

A read receipt lets an app report that a participant read the conversation. It contains no message reference or read position. Its timestamp is the message timestamp.

Type ID: `xmtp.org/readReceipt:1.0`. The payload is empty. It has no fallback. `shouldPush` defaults to `false` on Browser, Node, Kotlin, and Swift.

Read receipts are background events. Filter them from normal message lists. The SDK decodes a read receipt as the read-receipt content variant. It has no fallback text. A renderer that does not show receipts can skip this variant.
