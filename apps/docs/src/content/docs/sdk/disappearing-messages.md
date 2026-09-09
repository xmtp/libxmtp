---
title: Disappearing messages
---

Disappearing messages are messages that are intended to be visible to users for only a short period of time. After the message expiration time passes, the messages are removed from the UI and deleted from local storage so the messages are no longer accessible to conversation participants.

## Enable disappearing messages for a conversation

You can set disappearing message conditions when you create a group or DM, or update an existing conversation.

Conversation participants using apps that support disappearing messages will have a UX that honors the message expiration conditions. Conversation participants using apps that don't support disappearing messages won't experience disappearing message behavior.

Messages abide by the disappearing message settings for the conversation.

### Permissions for setting disappearing message conditions

Who can set or change disappearing message settings depends on the conversation type:

- **Group chats**: Only group admins can set or change disappearing message settings
- **DMs**: Both participants can set or change disappearing message settings

### Disappearing message settings

| Setting                 | Protocol field              | Meaning                                      |
| ----------------------- | --------------------------- | -------------------------------------------- |
| `disappearStartingAtNs` | `message_disappear_from_ns` | Timestamp from which the lifespan is counted |
| `retentionDurationInNs` | `message_disappear_in_ns`   | How long the message stays visible           |

A message expires at `disappearStartingAtNs + retentionDurationInNs`. Each message carries the result as `expiresAtNs`.

When you update disappearing message settings, the system changes each field separately under the hood. This means you'll receive two separate protocol messages when settings are updated:

1. One message for `message_disappear_from_ns` (the `disappearStartingAtNs` field)
2. One message for `message_disappear_in_ns` (the `retentionDurationInNs` field)

### Read and change the settings

| Action          | Browser, Node                                     | Kotlin, Swift                                 |
| --------------- | ------------------------------------------------- | --------------------------------------------- |
| Set at creation | `messageDisappearingSettings` option              | `disappearingMessageSettings` argument        |
| Update          | `updateMessageDisappearingSettings(fromNs, inNs)` | `updateDisappearingMessageSettings(settings)` |
| Clear           | `removeMessageDisappearingSettings()`             | `clearDisappearingMessageSettings()`          |
| Read            | `messageDisappearingSettings()`                   | `disappearingMessageSettings`                 |
| Check           | `isDisappearingMessagesEnabled()`                 | `isDisappearingMessagesEnabled()`             |

Clearing emits the same two protocol messages because it updates both fields to zero.

## Automatic deletion from local storage

A background worker sleeps until the next message expires. It then deletes all messages that are due. A new disappearing message wakes it so it can calculate a new deadline. With no scheduled expiry, it waits for 24 hours.

## Automatic removal from UI

Expired messages don't require manual removal from the UI. If your app UI updates when the local storage changes, expired messages will disappear automatically when the background worker deletes them from local storage.

## UX tips for disappearing messages

To ensure that users understand which messages are disappearing messages and their behavior, consider implementing:

- A distinct visual style: Style disappearing messages differently from regular messages (e.g., a different background color or icon) to indicate their temporary nature.
- A clear indication of the message's temporary nature: Use a visual cue, such as a timestamp or a countdown, to inform users that the message will disappear after a certain period.
