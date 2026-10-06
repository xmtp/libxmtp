---
title: Disappearing messages
---

Disappearing messages are messages that are intended to be visible to users for only a short period of time. After the expiration time, SDK queries hide the messages. A background worker deletes them from local storage. Your app must update its UI when state changes.

## Enable disappearing messages for a conversation

You can set disappearing message conditions when you create a group or DM, or update an existing conversation.

Conversation participants using apps that support disappearing messages will have a UX that honors the message expiration conditions. Conversation participants using apps that don't support disappearing messages won't experience disappearing message behavior.

Messages abide by the disappearing message settings for the conversation.

### Permissions for setting disappearing message conditions

Who can set or change disappearing message settings depends on the conversation type:

- **Group chats**: Admins and super admins can change the settings under either built-in permission set. A custom group policy can change who has permission
- **DMs**: Both participants can set or change disappearing message settings

### Disappearing message settings

| Setting       | Protocol field              | Meaning                                             |
| ------------- | --------------------------- | --------------------------------------------------- |
| `from`        | `message_disappear_from_ns` | Messages sent at or after this timestamp can expire |
| `retentionNs` | `message_disappear_in_ns`   | How long each eligible message remains visible      |

Both values must be positive to enable disappearing messages. An eligible message expires at `sentAt.ns + retentionNs`. The SDK clamps overflow to the largest signed 64-bit timestamp. Each message exposes its deadline as `expiresAt`, a `Timestamp` value.

When you update disappearing message settings, the system changes each field separately under the hood. This means you'll receive two separate protocol messages when settings are updated:

1. One message for `message_disappear_from_ns` (the `from` field)
2. One message for `message_disappear_in_ns` (the `retentionNs` field)

### Read and change the settings

Use the same fields and methods on all four SDKs:

| Action          | API                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------ |
| Set at creation | `CreateGroupOptions.disappearing` or `CreateDmOptions.disappearing`                                    |
| Update          | `updateDisappearingSettings(settings)`                                                                 |
| Clear           | `updateDisappearingSettings` with `undefined` on Browser and Node, `null` on Kotlin, or `nil` on Swift |
| Read            | `group.state().common.disappearingSettings` or `dm.state().disappearingSettings`                       |
| Check           | `group.state().common.isDisappearingEnabled` or `dm.state().isDisappearingEnabled`                     |

Read `state()` asynchronously. Swift uses the label `settings:` for updates. The settings value is a `DisappearingSettings` record with `from` and `retentionNs`.

Clearing emits the same two protocol messages because it updates both fields to zero.

## Automatic deletion from local storage

A background worker sleeps until the next message expires. It then deletes all messages that are due. A new disappearing message wakes it so it can calculate a new deadline. With no scheduled expiry, its default wait is 24 hours, plus any configured worker jitter.

## Automatic removal from UI

Expired messages are hidden from queries as soon as their deadline passes. Listen for `message.expired` [client events](/sdk/events/) and read the affected history again. The event is emitted after local cleanup, which can happen after the deadline. Use a UI timer if the message must disappear at the deadline.

## UX tips for disappearing messages

To ensure that users understand which messages are disappearing messages and their behavior, consider implementing:

- A distinct visual style: Style disappearing messages differently from regular messages (e.g., a different background color or icon) to indicate their temporary nature.
- A clear indication of the message's temporary nature: Use a visual cue, such as a timestamp or a countdown, to inform users that the message will disappear after a certain period.
