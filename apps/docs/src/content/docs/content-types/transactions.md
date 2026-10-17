---
title: Wallet transactions
---

Wallet send calls ask a wallet to execute one or more chain calls.

Type ID: `xmtp.org/walletSendCalls:1.0`. The payload is JSON-encoded `WalletSendCalls`. Its fallback contains `[Transaction request generated]:` followed by the JSON payload. `shouldPush` defaults to `true` on Browser, Node, Kotlin, and Swift.

| Field          | Meaning                             |
| -------------- | ----------------------------------- |
| `version`      | Wallet call protocol version        |
| `chainId`      | Hex chain ID, such as `0x1`         |
| `from`         | Sender account                      |
| `calls`        | Calls for the wallet to execute     |
| `capabilities` | Optional wallet capability requests |

Each call can contain a target address, value, call data, gas limit, and metadata. These fields are all optional. After execution, send a [transaction reference](/content-types/transaction-refs/) so the conversation can follow the result.

| Call field | Meaning                     |
| ---------- | --------------------------- |
| `to`       | Optional target address     |
| `value`    | Optional native token value |
| `data`     | Optional encoded call data  |
| `gas`      | Optional gas limit          |
| `metadata` | Optional display metadata   |

| SDK metadata field | Meaning                                 |
| ------------------ | --------------------------------------- |
| `description`      | Text that describes the action          |
| `transactionType`  | Operation, such as `transfer` or `lend` |
| `extra`            | Map of additional string values         |

The codec writes `extra` entries as fields in the wire JSON metadata object. They are not a fixed set of transaction fields. `capabilities` is an optional map of string values.

The Agent SDK also provides `getERC20Decimals`, `getERC20Balance`, and `createERC20TransferCalls` helpers through `@xmtp/agent-sdk/util`.
