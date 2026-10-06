---
title: Send messages
---

Once you have the group chat or DM conversation, you can send messages in the conversation.

Sending does not update your own UI. Stream the conversation, or send optimistically, to display your own message.

All four SDKs have `sendText` for text and `send` for encoded content. Browser and Node also accept a typed codec and its value in `send(codec, value, options)`. Kotlin and Swift provide the same typed-codec overload.

| Platform      | Send text                                             | Send encoded content                                    |
| ------------- | ----------------------------------------------------- | ------------------------------------------------------- |
| Browser, Node | `conversation.sendText(text, options?)`               | `conversation.send(encoded, options?)`                  |
| Kotlin        | `conversation.sendText(text, options)`                | `conversation.send(encoded, options)`                   |
| Swift         | `conversation.sendText(text: text, options: options)` | `conversation.send(encoded: encoded, options: options)` |

Every form returns the message ID.

## Control push notifications for a message

`shouldPush` decides whether a message triggers a push notification on recipient devices. See [Push notifications](/sdk/push-notifications/).

Standard content uses the catalogue push default. Custom content can use the codec push default. Set `SendOptions.shouldPush` to override the default with any send form:

| Platform      | Example                                                         |
| ------------- | --------------------------------------------------------------- |
| Browser, Node | `sendText(text, { shouldPush: false })`                         |
| Kotlin        | `sendText(text, SendOptions(shouldPush = false))`               |
| Swift         | `sendText(text: text, options: SendOptions(shouldPush: false))` |

## Optimistically send messages

Optimistic sending returns a message ID before publication completes. Use the local message to update the sender's UI.

All four SDKs accept `SendOptions.optimistic`. Set it to `true` to store the message and queue its send, then return before publication completes. You can also call `prepareMessage(encoded, options)` to prepare encoded content. Browser and Node also accept a typed codec and value in `prepareMessage`.

Call `publishMessages()` to publish prepared messages, or `publishMessage(id)` to publish one message. Swift uses the `id:` label for `publishMessage`.

`SendOptions` also accepts `idempotencyKey`. Re-sending identical content with the same key produces the same message ID.

### Key UX considerations for optimistically sent messages

- After optimistically sending a message, show the user an indicator that the message is still being processed. After successfully sending the message, show the user a success indicator.
  - An optimistically sent message initially has an `unpublished` status. Once published, it has a `published` status. You can use this status to determine which indicator to display in the UI.
- If an optimistically sent message fails to send it will have a `failed` status. In this case, be sure to give the user an option to retry sending the message or cancel sending. Use a try/catch block to intercept errors and allow the user to retry or cancel.

### Complete publication of optimistic messages

Call `publishMessages()` to process the pending send intents and wait for publication. Call `publishMessage` with one message ID to queue or retry that stored message. Both methods can report errors.

`prepareMessage` also queues a send intent. The public SDK does not expose a `noSend` parameter to hold publication. Validate a [remote attachment](/content-types/attachments/) upload before you send its content.

Use `client.conversations.deleteMessageLocally(id)` on Browser and Node, or `client.conversations().deleteMessageLocally` on Kotlin and Swift to remove local content. You can also call `deleteLocally()` on a loaded message. A local deletion does not cancel a send that is already queued.
