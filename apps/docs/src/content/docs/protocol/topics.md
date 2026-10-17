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

> **Note on [DM stitching](/sdk/push-notifications/#dm-stitching):** A DM can contain several underlying groups. The notification sync task subscribes to every matching group automatically.

The welcome message topic is used to deliver a `Welcome` message to a new member of a group. This message bootstraps the new member, providing them with the group's state so they can participate.

- **Purpose**: To notify a specific app installation that it has been added to a new conversation.

Key package topics do not support normal queries. Use a newest-envelope read.

## Topic text and push payloads

`conversation.topic` is a display string, such as
`[group_message_v1/00112233445566778899aabbccddeeff]`. Do not send this display
string as a binary wire topic.

Push payloads encode the complete binary wire topic with standard Base64. The
JSON `sequence_id` is decimal text to preserve integer precision. Decode the
`topic` field before you use it as a wire topic. Only group-message and Welcome
topics support push subscriptions.

The old `/xmtp/mls/1/g-.../proto` and `/xmtp/mls/1/w-.../proto` labels are not the
self-hosted push format. Update receivers to use the
[current payload](/sdk/push-notifications/#payload).
