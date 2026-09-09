---
title: Wallet transactions
---

Wallet send calls ask a wallet to execute one or more chain calls.

Type ID: `xmtp.org/walletSendCalls:1.0`. The payload is a `WalletSendCalls` protobuf. It has no fallback. `shouldPush` defaults to `true` on Browser, Node, and Kotlin. Swift does not include this codec.

| Field          | Meaning                             |
| -------------- | ----------------------------------- |
| `version`      | Wallet call protocol version        |
| `chainId`      | Hex chain ID, such as `0x1`         |
| `from`         | Sender account                      |
| `calls`        | Calls for the wallet to execute     |
| `capabilities` | Optional wallet capability requests |

Each call contains a target address, optional value, call data, and optional metadata. After execution, send a [transaction reference](/content-types/transaction-refs/) so the conversation can follow the result.

| Call field | Meaning                     |
| ---------- | --------------------------- |
| `to`       | Target address              |
| `value`    | Optional native token value |
| `data`     | Encoded call data           |
| `gas`      | Optional gas limit          |
| `metadata` | Optional display metadata   |

| Metadata field    | Meaning                                 |
| ----------------- | --------------------------------------- |
| `description`     | Human-readable action                   |
| `transactionType` | Operation, such as `transfer` or `lend` |
| `currency`        | Asset symbol                            |
| `amount`          | Amount in the asset's smallest unit     |
| `decimals`        | Asset decimal places                    |
| `toAddress`       | Optional displayed recipient            |
| `platform`        | Optional protocol or application        |
| `apy`             | Optional displayed yield                |

The Agent SDK also provides `getERC20Decimals`, `getERC20Balance`, `createERC20TransferCalls`, and `validHex` helpers.
