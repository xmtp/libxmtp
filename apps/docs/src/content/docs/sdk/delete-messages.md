---
title: Delete messages
---

A user can delete messages they sent in a DM or group chat. In a group chat, the super admin role can delete any message in the group.

:::tip
Message deletion is not a security or privacy feature. It provides a best-effort way to remove messages from conversation UIs, but does not guarantee the message content is permanently erased.
:::

## How message deletion works

When a user deletes a message, a `DeleteMessage` content type containing the target message ID is sent to the conversation. Clients receiving this message validate the deletion request and filter the deleted message from queries.

When you query messages, the client automatically:

- Replaces deleted messages with a placeholder that indicates whether the sender or an admin deleted it
- Filters out the `DeleteMessage` content type from message lists

The deletion mechanism does NOT remove the original message from:

- Local databases
- The backend
- Backup systems

## What cannot be deleted

| Cannot be deleted                    | Reason                                             |
| ------------------------------------ | -------------------------------------------------- |
| Membership changes and group updates | They are the group transcript                      |
| Leave requests                       | They are the group transcript                      |
| Reactions                            | They are deleted with their target                 |
| Read receipts                        | They have no content to remove                     |
| Actions and intents                  | They are not user-authored content                 |
| A deletion                           | Removing it would hide the original with no record |
| An unknown content type              | The SDK does not silently drop unknown data        |

## Delete a message

| Platform      | Method                                                      |
| ------------- | ----------------------------------------------------------- |
| Swift         | `conversation.deleteMessage(messageId:)`                    |
| Kotlin        | `conversation.deleteMessage(messageId)`                     |
| Browser, Node | Not available. These SDKs can receive and render deletions. |

The call throws when the message is absent, the caller is not the sender or a super admin, the message is already deleted, or its type cannot be deleted.

## Stream deletions

| Platform      | Method                                          | Yields          |
| ------------- | ----------------------------------------------- | --------------- |
| Browser, Node | `client.conversations.streamDeletedMessages()`  | Deleted message |
| Kotlin, Swift | `client.conversations.streamMessageDeletions()` | Deleted message |

The deprecated Browser and Node `streamMessageDeletions()` method yields only the message ID.
