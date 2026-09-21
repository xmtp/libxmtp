---
xip: 58
title: Disappearing messages
description: Proposal to support disappearing messages that are intended to be visible to users for only a short period of time.
author: Mojtaba Chenani (@mchenani)
discussions-to: https://community.xmtp.org/t/xip-57-disappearing-messages/888
status: Draft
type: Standards
category: XRC
created: 2025-02-13
---

## Abstract

Disappearing messages are messages that are intended to be visible to users for only a short period of time. After the message expiration time passes, the messages are removed from the UI and deleted from local storage so the messages are no longer accessible to conversation participants.

## Motivation

As a modern messaging protocol, XMTP must support disappearing messages—a feature that users and app developers increasingly expect as standard in secure and private communication platforms. Ephemeral messaging has evolved from a novel feature to an essential capability that allows users to maintain control over their digital footprint. This aligns with XMTP's commitment to privacy and security, complementing the protocol's existing encryption and security measures provided through XMTP and MLS.

## Specification

Disappearing message behavior will enforced by apps, meaning that apps are responsible for removing messages from their UIs and local storage based on conditions set at the conversation level. Conversation participants using apps that support disappearing messages will have a UX that honors the message expiration conditions.

It's important to understand that:

- A conversation participant using an app that doesn't support disappearing messages won't experience disappearing message behavior.
- Messages aren't deleted from the XMTP network.

Therefore, disappearing messages should be understood as best-effort, app-level privacy that helps avoid leaving an easily accessible record in a messaging UI. However, it's not a guaranteed, system-wide “message self-destruct.”

### Enable disappearing messages for a conversation

When creating or updating a conversation, only group admins and DM participants will be able to set disappearing message expiration conditions.

This includes setting the following conditions expressed in nanoseconds (ns):

- `disappearStartingAtNs`: Starting timestamp from which the message lifespan is calculated
- `retentionDurationInNs`: Duration of time during which the message should remain visible to conversation participants

For example:

1. Set `disappearStartingAtNs` to the current time, such as `1738620126404999936` (nanoseconds since the Unix epoch of January 1, 1970).
2. Set `retentionDurationInNs` to the message lifespan, such as 1800000000000000 (30 minutes).
3. Use `disappearStartingAtNs` and `retentionDurationInNs` to calculate the message expiration time of `1738620126404999936 + 1800000000000000 = 1740420126404999936`.

### Set disappearing message setting on conversation create

When sending a message, it abides by message expiration conditions set for the conversation. For example:

```tsx
// DM
await client.conversations.newConversation(
    address,
    {
        disappearingMessageSettings: DisappearingMessageSettings(
            disappearStartingAtNs: 1738620126404999936,
            retentionDurationInNs: 1800000000000000
        )
    }
);

// Group
await client.conversations.newGroup(
    [address],
    {
        disappearingMessageSettings: DisappearingMessageSettings(
            disappearStartingAtNs: 1738620126404999936,
            retentionDurationInNs: 1800000000000000
        )
    }
);
```

### Update disappearing message setting on an existing conversation

When sending a message, it will abide by message expiration conditions set for the conversation. For example:

```tsx
await conversation.updateDisappearingMessageSettings(updatedSettings)
await conversation.clearDisappearingMessageSettings()
```

### Get a conversation’s disappearing message setting

When sending a message, it will abide by message expiration conditions set for the conversation. For example:

```tsx
conversation.disappearingMessageSettings
conversation.isDisappearingMessagesEnabled()
```

### Automatic deletion from local storage

A background worker will run every one second to clean up expired disappearing messages. The worker will automatically delete expired messages from local storage. No additional action will be required by integrators.

### Automatic removal from UI

Expired messages won’t require manual removal from the UI. If an app UI updates when the local storage changes, expired messages will disappear automatically when the background worker deletes them from local storage.

### Receive a disappearing message

On the receiving side, an app doesn't need to check expiration conditions manually. It can receive and process messages as usual, and the background worker will handle message expiration cleanup.

## Rationale

Some users, depending on their needs, want disappearing messages in a group or one-on-one conversation to ensure messages don’t linger longer than expected on other participants’ devices. Enabling this feature provides a way for users to enforce message expiration, knowing that as long as all clients are updated and functioning correctly, the messages will be automatically removed from those clients.

## Backward compatibility

Since older clients can’t process the new metadata changes, messages won’t be deleted from those clients. Also, those clients won’t recognize that disappearing messages are enabled. This means they will continue sending and displaying messages normally, but for users with updated clients, those messages will be deleted based on the expiration settings—without the sender necessarily being aware that their messages have disappeared.

## Test cases

See tests in [mls.rs](https://github.com/xmtp/libxmtp/blob/8771b149338fba17dd0e1fb97f6eb11bc7ba6491/bindings_ffi/src/mls.rs#L5200-L5453) in the libxmtp repo.

## Reference implementation

See [conversation.rs](https://github.com/xmtp/libxmtp/blob/main/bindings_node/src/conversation.rs#L698).

To learn more about the background worker, see [disappearing_messages.rs](https://github.com/xmtp/libxmtp/blob/main/xmtp_mls/src/groups/disappearing_messages.rs#L68).

## Security considerations

### Threat model

A malicious but updated client could still prevent messages from being deleted on their side. They would receive the disappearing message along with the expiration settings but could modify or build a custom client that ignores the deletion process and retains expired messages.

However, this isn’t a major concern because, ultimately, a recipient can always copy and store a message manually if they choose to. Disappearing messages are more about reducing accessibility and limiting exposure rather than providing absolute protection against message retention.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
