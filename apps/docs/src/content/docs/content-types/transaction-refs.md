---
title: Transaction references
---

A transaction reference points to a transaction that was submitted on a chain.

Type ID: `xmtp.org/transactionReference:1.0`. The payload is JSON-encoded `TransactionReference`. Its fallback includes the transaction reference, or `Crypto transaction` when the reference is empty. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Field       | Meaning                        |
| ----------- | ------------------------------ |
| `namespace` | Optional transaction namespace |
| `networkId` | Network identifier             |
| `reference` | Transaction hash or identifier |
| `metadata`  | Optional transaction metadata  |

The Agent SDK emits the `transaction-reference` event. Use `ctx.conversation.sendTransactionReference()` to send one.

The optional metadata contains `transactionType`, `currency`, `amount`, `decimals`, `fromAddress`, and `toAddress`. `amount` is a floating-point number. The codec does not convert units or verify the transaction. Agree on amount units with the receiving app. The SDK `networkId` field is a string; the decoder also accepts a JSON number and converts it to decimal text.
