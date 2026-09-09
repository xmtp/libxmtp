---
title: Agent context
---

Every Agent SDK handler receives a context. The available surface depends on the event.

| Property or method                     | Purpose                                       |
| -------------------------------------- | --------------------------------------------- |
| `client`                               | Node SDK client                               |
| `conversation`                         | Current conversation                          |
| `message`                              | Current decoded message, for message events   |
| `getClientAddress()`                   | Current client's account identifier           |
| `isDm()`, `isGroup()`                  | Narrow the conversation type                  |
| `isAllowed`, `isDenied`, `isUnknown`   | Read conversation consent state               |
| `sendRemoteAttachment(file, callback)` | Encrypt, upload, and send a file              |
| `usesCodec(Codec)`                     | Narrow custom content                         |
| `isMarkdown()`, `isText()`             | Narrow text content                           |
| `isReply()`, `isReaction()`            | Narrow reply or reaction content              |
| `isReadReceipt()`                      | Narrow read-receipt content                   |
| `isRemoteAttachment()`                 | Narrow remote-attachment content              |
| `isTransactionReference()`             | Narrow transaction-reference content          |
| `isWalletSendCalls()`                  | Narrow wallet-call content                    |
| `sendReaction(content, schema?)`       | React to the current message                  |
| `sendMarkdownReply(markdown)`          | Reply with Markdown                           |
| `sendTextReply(text)`                  | Reply with text                               |
| `getSenderAddress()`                   | Resolve the sender's first account identifier |

```ts source="agents-context-1.ts" region="example1"

```

The context exposes the underlying conversation. Use its SDK methods for members, messages, metadata, and content-type send helpers.
