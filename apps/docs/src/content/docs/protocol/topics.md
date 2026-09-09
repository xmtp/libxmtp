---
title: Topics
---

While XMTP SDKs manage topic subscriptions automatically, understanding them can be helpful for protocol-level development, debugging, and building services like push notification servers.

## Wire topics

A topic addresses one stream of envelopes. It is one kind byte followed by the identifier bytes for that kind.

| Kind   | Payload          | Identifier               |
| ------ | ---------------- | ------------------------ |
| `0x00` | Group message    | 16-byte group ID         |
| `0x01` | Welcome          | 32-byte installation key |
| `0x02` | Identity update  | 32-byte inbox ID         |
| `0x03` | Key package      | 32-byte installation key |
| `0x04` | Commit-log entry | 16-byte group ID         |

You never send a topic when you publish. The backend derives the topic from the payload. Reads and subscriptions name the topics to retrieve. An unknown kind or an identifier with the wrong length returns `INVALID_ARGUMENT`.

The group message topic is used to send and receive messages within a specific conversation (both 1:1 DMs and group chats). Each conversation has its own unique topic.

- **Purpose**: Carries all ongoing communication for a conversation, including [application messages](/protocol/envelope-types/#group-messages) (text, reactions, etc.) and [commit messages](/protocol/envelope-types/#group-messages) that modify the group state.

> **Note on [DM stitching](/sdk/push-notifications/#dm-stitching):** For direct messages, multiple underlying conversations might be "stitched" together in the UI. For push notifications to be reliable, an app must subscribe to the group message topic for each of these underlying conversations.

The welcome message topic is used to deliver a `Welcome` message to a new member of a group. This message bootstraps the new member, providing them with the group's state so they can participate.

- **Purpose**: To notify a specific app installation that it has been added to a new conversation.

Key package topics do not support normal queries. Use a newest-envelope read.

## Push routing labels

Push code uses text labels that are different from wire topics.

| Label                                  | Source                     |
| -------------------------------------- | -------------------------- |
| `/xmtp/mls/1/g-{groupId}/proto`        | `conversation.topic`       |
| `/xmtp/mls/1/w-{installationId}/proto` | Installation Welcome label |

These strings are push routing labels. They do not convert to or from binary wire topics. Only group messages and Welcomes have push routing labels.
