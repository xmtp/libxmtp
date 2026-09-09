---
title: Transaction references
---

A transaction reference points to a transaction that was submitted on a chain.

Type ID: `xmtp.org/transactionReference:1.0`. The payload is a `TransactionReference` protobuf. It has no fallback. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Field       | Meaning                        |
| ----------- | ------------------------------ |
| `namespace` | Transaction namespace          |
| `networkId` | Network identifier             |
| `reference` | Transaction hash or identifier |
| `metadata`  | Optional transaction metadata  |

The Agent SDK emits the `transaction-reference` event and provides `sendTransactionReference()`.

Transaction amounts in metadata are integer base units. Divide by `10^decimals` for display. For example, `100000` base units with 6 decimals is `0.1` tokens.
