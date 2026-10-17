---
title: Agent context
---

Message events receive `MessageContext`. Conversation events receive `ConversationContext`. `start` and `stop` receive `ClientContext`. `unhandledError` receives an `Error`. Error middleware receives a partial context with a client and optional message and conversation.

| Property or method                         | Purpose                                                   |
| ------------------------------------------ | --------------------------------------------------------- |
| `client`                                   | Node SDK client                                           |
| `conversation`                             | Current conversation                                      |
| `message`                                  | SDK message with tagged content, for message events       |
| `content`                                  | Decoded value, for message events                         |
| `getClientAddress()`                       | Current client's account identifier                       |
| `isDm()`, `isGroup()`                      | Narrow the conversation type                              |
| `isAllowed()`, `isDenied()`, `isUnknown()` | Read conversation consent state with `await`              |
| `sendRemoteAttachment(file, callback?)`    | Use backend attachment hosting, or an app upload callback |
| `usesCodec(Codec)`                         | Narrow custom content                                     |
| `isMarkdown()`, `isText()`                 | Narrow text content                                       |
| `isReply()`, `isReaction()`                | Narrow tagged reply or reaction content                   |
| `isReadReceipt()`                          | Narrow read-receipt content                               |
| `isRemoteAttachment()`                     | Narrow remote-attachment content                          |
| `isTransactionReference()`                 | Narrow transaction-reference content                      |
| `isWalletSendCalls()`                      | Narrow wallet-call content                                |
| `sendReaction(content, schema?)`           | React to the current message                              |
| `sendMarkdownReply(markdown)`              | Reply with Markdown                                       |
| `sendTextReply(text)`                      | Reply with text                                           |
| `getSenderAddress()`                       | Resolve the sender's first account identifier             |

```ts source="agents-context-1.ts" region="example1"

```

The context exposes the underlying conversation. Use its SDK methods for members, messages, metadata, and content-type send helpers.

For a text event, read the string from `ctx.content`. `ctx.message.content` is a tagged record, such as `{ kind: "text", value: "hello" }`. For a reply, `ctx.content` is the reply record with `referenceId` and `body`. Read-receipt contexts have `undefined` content.

Assigning `ctx.content` changes the value used by later middleware. It does not change `ctx.message`. The command router uses this property to pass command arguments to its handler.

`sendReaction()`, `sendTextReply()`, and `sendMarkdownReply()` disable push notifications. Use the underlying conversation send methods when you need other send options.
