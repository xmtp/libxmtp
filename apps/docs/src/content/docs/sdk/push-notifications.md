---
title: Push notifications
---

XMTP does not deliver push notifications. You run a push service that watches topics, filters messages, and sends accepted notifications through Apple Push Notification service, Firebase Cloud Messaging, or Web Push.

## The three-stage filter

| Stage | Check              | Drop when                               |
| ----- | ------------------ | --------------------------------------- |
| 1     | Topic subscription | The topic is not subscribed             |
| 2     | `shouldPush`       | The flag is false                       |
| 3     | Sender HMAC        | It matches one of the user's group keys |

The third stage prevents notifications for the user's own messages without decrypting their content.

| Default | Content types                                                                                     |
| ------- | ------------------------------------------------------------------------------------------------- |
| `true`  | Text, attachments, replies, markdown, actions, intents, transaction references, wallet send calls |
| `false` | Reactions, read receipts, group updates, membership changes, deletions, leave requests            |

## HMAC keys

Each inbox has a 42-byte root HMAC key. Each conversation key is derived for one 30-day period. The previous, current, and next period keys are live at the same time.

A user holds the HMAC keys for any conversation they join, but an outside observer only sees the keys without knowing who owns them. For instance, suppose Alix has HMAC key #1, and we also see HMAC keys #2 and #3. If Alix discloses that they hold key #1, then we know key #1 belongs to them. However, we have no way of knowing who holds keys #2 or #3 unless those individuals reveal that information. This design preserves privacy while enabling secure communication.

Device sync distributes a rotated root key. A stale installation derives different group keys, so the sender filter fails and the user receives notifications for their own messages. Watch `preferences.streamPreferences` on Browser and Node or `streamPreferenceUpdates` on Kotlin and Swift. Fetch keys and update subscriptions after an HMAC-key event.

## Receive a notification

| Platform                              | Can decrypt before display?                         |
| ------------------------------------- | --------------------------------------------------- |
| Android                               | Yes                                                 |
| Web                                   | The SDK lacks the complete topic and processing API |
| iOS with the filtering entitlement    | Yes                                                 |
| iOS without the filtering entitlement | No                                                  |

Kotlin and Swift can process a welcome with `conversations.fromWelcome(bytes)`. For a conversation message, find the conversation by topic, call `sync()`, and then call `processMessage(bytes)`. Browser and Node do not provide these APIs.

## Topics

The push-routing topic strings are not backend wire topics.

| Topic        | Format                                |
| ------------ | ------------------------------------- |
| Welcome      | `/xmtp/mls/1/w-$installationId/proto` |
| Conversation | `/xmtp/mls/1/g-$conversationId/proto` |

Kotlin and Swift provide `welcomeTopic`, `getPushTopics`, `allPushTopics`, and `getHmacKeys`. Browser and Node provide `conversations.hmacKeys()` but do not provide push-topic helpers.

Subscribe to the welcome topic and every conversation topic. The welcome subscription is needed before the installation joins a new conversation.

## DM stitching

Duplicate DM conversations can occur when a user creates new conversations with the same contact from different installations or devices.

The SDK presents the underlying conversations as one DM. A push service must subscribe to every topic returned by `getPushTopics()` because each underlying conversation keeps its own topic and HMAC keys. Drop duplicate-DM welcome notifications because the user already has the visible conversation.

When fetching messages from any of the MLS groups associated with a DM conversation, the XMTP SDK responds with messages from all of the groups. When sending messages in a DM conversation, all installations in that DM will eventually converge on whichever underlying conversation was used last.

While a DM conversation can have multiple topics, each individual message is sent to only one topic. This means one message produces at most one push notification.

A welcome message is sent when another underlying DM is added to the stitched DM. These welcome messages are filtered out of SDK streams, but they are not filtered out for your push service. Drop the duplicate welcome because the user already has the conversation.
