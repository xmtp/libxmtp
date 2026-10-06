---
title: Agent events
---

Subscribe only to the events that your agent needs. The Agent SDK uses Node's `EventEmitter` interface.

| Event                   | Value                                              |
| ----------------------- | -------------------------------------------------- |
| `message`               | Each received message after middleware             |
| `text`                  | Plain text                                         |
| `markdown`              | Markdown text                                      |
| `attachment`            | Remote attachment                                  |
| `inline-attachment`     | Inline attachment                                  |
| `multi-attachment`      | Multiple remote attachments                        |
| `reaction`              | Reaction                                           |
| `reply`                 | Tagged reply; enrichment on the message            |
| `read-receipt`          | Read receipt                                       |
| `actions`               | Actions prompt                                     |
| `intent`                | Selected action intent                             |
| `transaction-reference` | Transaction reference                              |
| `wallet-send-calls`     | Wallet call request                                |
| `group-update`          | Group membership or metadata update                |
| `leave-request`         | Leave request                                      |
| `conversation`          | Conversation first discovered by this client       |
| `dm`                    | New direct message conversation                    |
| `group`                 | New group conversation                             |
| `start`, `stop`         | Agent lifecycle                                    |
| `unhandledError`        | Unhandled error                                    |
| `unknownMessage`        | Unknown or custom content without a built-in event |

```ts source="agents-events-1.ts" region="example1"

```

:::caution
The Agent skips messages from its own inbox. For other messages, it runs middleware, emits the content event, then emits `message`. Middleware that returns without `next()` suppresses both events. Filter by content before you respond to `message`. Group updates and other background messages can also reach it.
:::

The Agent awaits async message and conversation listeners. Return or await all work that must finish before message delivery is accepted. A task that runs without `await` can continue after acceptance.

Conversation events exclude conversations already stored locally. They include a pending Welcome when the client first discovers its conversation.

`start` means that local readers are ready. It does not confirm a backend connection. Retryable network errors use the core recovery budget. A terminal stream error stops both readers. Handling the error does not restart them. Call `agent.start()` to start a new reader generation. Use `onConnectionStateChange` in the `start()` options to observe connection state.

Agent message events differ from the [SDK client events](/sdk/events/). Use `agent.client` to listen for local storage, consent, and message-status changes.
