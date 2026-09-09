---
title: Send messages
---

Once you have the group chat or DM conversation, you can send messages in the conversation.

Sending does not update your own UI. Stream the conversation, or send optimistically, to display your own message.

| Platform      | Send text                                              | Send encoded content                                   |
| ------------- | ------------------------------------------------------ | ------------------------------------------------------ |
| Node, Browser | `conversation.sendText(text, opts?)`                   | `conversation.send(encodedContent, opts?)`             |
| Swift         | `conversation.send(content:options:)`                  | `conversation.send(encodedContent:visibilityOptions:)` |
| Kotlin        | `conversation.send(text)` or `send(content, options?)` | `conversation.send(encodedContent, opts?)`             |

Every form returns the message ID.

## Control push notifications for a message

`shouldPush` decides whether a message triggers a push notification on recipient devices. See [Push notifications](/sdk/push-notifications/#the-three-stage-filter).

The text and content forms derive `shouldPush` from the content codec. To override it, encode the content and use the encoded-content form:

| Platform      | Override                                                            |
| ------------- | ------------------------------------------------------------------- |
| Browser, Node | `send(encodedContent, { shouldPush: false })`                       |
| Kotlin, Swift | `send(encodedContent, MessageVisibilityOptions(shouldPush: false))` |

## Optimistically send messages

Optimistic sending displays the message in the sender's UI immediately and processes it in the background.

| Platform      | Send locally                                 | Publish all         | Publish one                 |
| ------------- | -------------------------------------------- | ------------------- | --------------------------- |
| Browser, Node | `sendText(text, { optimistic: true })`       | `publishMessages()` | Not available               |
| Kotlin, Swift | `prepareMessage(content, options?, noSend?)` | `publishMessages()` | `publishMessage(messageId)` |

Browser and Node have no `prepareMessage`. `SendOpts` also accepts `idempotencyKey`. Re-sending identical content with the same key produces the same message ID.

### Key UX considerations for optimistically sent messages

- After optimistically sending a message, show the user an indicator that the message is still being processed. After successfully sending the message, show the user a success indicator.
  - An optimistically sent message initially has an `unpublished` status. Once published, it has a `published` status. You can use this status to determine which indicator to display in the UI.
- If an optimistically sent message fails to send it will have a `failed` status. In this case, be sure to give the user an option to retry sending the message or cancel sending. Use a try/catch block to intercept errors and allow the user to retry or cancel.

### Control publication of optimistic messages

By default, `publishMessages()` publishes all prepared messages. For more control, use the `noSend` parameter when preparing a message. The message won't be published until you explicitly call `publishMessage(messageId)`.

This is useful when sending [remote attachments](/content-types/attachments/). You can validate that the attachment upload succeeded before publishing. If the upload failed, you can choose to delete the local prepared message instead of publishing it.

On Kotlin and Swift, `deleteMessageLocally` is on `client.conversations`, not on the conversation.
