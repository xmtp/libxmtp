---
title: Agent events
---

Subscribe only to the events that your agent needs. The Agent SDK uses Node's `EventEmitter` interface.

| Event                   | Value                               |
| ----------------------- | ----------------------------------- |
| `message`               | Every incoming message              |
| `text`                  | Plain text                          |
| `markdown`              | Markdown text                       |
| `attachment`            | Remote attachment                   |
| `inline-attachment`     | Inline attachment                   |
| `multi-attachment`      | Multiple remote attachments         |
| `reaction`              | Reaction                            |
| `reply`                 | Enriched reply                      |
| `read-receipt`          | Read receipt                        |
| `actions`               | Actions prompt                      |
| `intent`                | Selected action intent              |
| `transaction-reference` | Transaction reference               |
| `wallet-send-calls`     | Wallet call request                 |
| `group-update`          | Group membership or metadata update |
| `leave-request`         | Leave request                       |
| `conversation`          | New conversation                    |
| `dm`                    | New direct message conversation     |
| `group`                 | New group conversation              |
| `start`, `stop`         | Agent lifecycle                     |
| `unhandledError`        | Unhandled error                     |
| `unknownMessage`        | Message with no specific event      |

```ts source="agents-events-1.ts" region="example1"

```

:::caution
The `message` event fires for every incoming message. Filter by content and sender before you respond. This prevents loops and responses to background events.
:::
