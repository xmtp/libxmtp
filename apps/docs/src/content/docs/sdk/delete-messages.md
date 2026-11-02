---
title: Delete messages
---

A user can delete messages they sent in a DM or group chat. In a group chat, a super admin can also delete another member's message. The content type must be deletable.

:::tip
Message deletion is not a security or privacy feature. It provides a best-effort way to remove messages from conversation UIs, but does not guarantee the message content is permanently erased.
:::

## How message deletion works

When a user deletes a message, a `DeleteMessage` content type containing the target message ID is sent to the conversation. Each installation, the sender's included, validates the deletion when it processes it from the conversation. It then marks the target as deleted and removes its content from the local database. An archive carries the mark, so a restore does not bring the content back.

When you query messages, the client automatically:

- Replaces deleted messages with a placeholder that indicates whether the sender or a super admin deleted it
- Filters out the `DeleteMessage` content type from message lists

The deletion mechanism does NOT remove the original message from:

- The backend
- Copies an app already received or exported
- Freed pages of the local database file

## What cannot be deleted

| Cannot be deleted                    | Reason                                             |
| ------------------------------------ | -------------------------------------------------- |
| Membership changes and group updates | They are the group transcript                      |
| Leave requests                       | They are the group transcript                      |
| Reactions                            | Queries hide them when their target is deleted     |
| Read receipts                        | They have no content to remove                     |
| Actions and intents                  | They are not user-authored content                 |
| A deletion                           | Removing it would hide the original with no record |
| An unknown content type              | The SDK does not silently drop unknown data        |

## Delete a message

| Platform                    | Method                                      |
| --------------------------- | ------------------------------------------- |
| Browser, Node, React Native | `conversation.deleteMessage(messageId)`     |
| Kotlin                      | `conversation.deleteMessage(messageId)`     |
| Swift                       | `conversation.deleteMessage(id: messageId)` |

You can also call `delete()` on a loaded message. Each form queues the deletion and returns the ID of the deletion message. The deletion publishes with the conversation's next publish or sync, and the target changes when it is processed.

A message that this installation stored and has not sent is cancelled instead: it and its queued send are removed, nothing is published, and the call returns the message's own ID. If its send is already in flight, the call throws an `InvalidInput` error with code `MessageInFlight`; retry after the send completes.

The call throws when the message is absent, the caller is not the sender or a super admin, the message is already deleted, or its type cannot be deleted.

## Observe deletions

Use the [client event stream](/sdk/events/) and select the `message.deleted` event kind. The event contains the conversation ID, target message ID, and deletion cause. Read the affected message again to update the UI. Events carry IDs and state values, not message content.

The cause distinguishes a conversation deletion from a local deletion. `message.expired` is a separate event for disappearing-message cleanup. The SDKs do not have `streamDeletedMessages` or `streamMessageDeletions` methods.
