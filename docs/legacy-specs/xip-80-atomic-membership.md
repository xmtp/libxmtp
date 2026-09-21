---
xip: 80
title: Atomic membership
description: A scaling proposal to limit the number of installations in a group for an inbox
author: Dakota Brink (@codabrink)
discussions-to: https://improve.xmtp.org/t/xip-80-atomic-membership/2072
status: Draft
type: Standards
category: XRC
created: 2026-01-28
---

## Abstract

This XIP provides a scaling proposal to limit the number of installations in a group for an inbox. It unlocks the ability for devs to have more than one instance of an agent online at a time. It also reduces commit size, reduces the number of commits, and increases security for XMTP's extra privacy-conscious users.

## Motivation

In the current XMTP, every installation of every inbox is a part of every group they’re a member of. This is generally ideal for the average user, but for agents this is a problem when you have more than one agent running. N bots will receive every message, replying n times to every prompt, along with bloating group size with unnecessary installations in the group. In a group, you really only need one agent installation in a group at a time.

## Specification

### New leaf node extension: Atomic membership

Here’s what the extension data will look like. It’ll be serialized into a proto-buf in the extension data.

```rust
struct AtomicMembershipExtension {
    flags: u8
}
```

If `flags & 1 == 1` (first bit is 1) in any inbox installation, other inboxes will only ensure that at least one active installation is present in the group. It’s expected that atomic inboxes will largely manage their own installations in groups.

When other inboxes search for which installation to add, they will only randomly select from a pool of installations where `flags & 2 == 2` (second bit is 1). This allows installations to turn themselves off from being added to new groups. If all installations have this bit off, then this bit is ignored.

**Possibly:** The third flag (`flags & 4 == 4`) tells the other users to select the oldest valid installation instead of a random installation. (Details at the end)

### Example scenarios

1. Adding an atomic inbox to a group

    ```mermaid
    graph TD
        ADD[Alix wishes to add Bo. Alix downloads Bo's KPs]
            ADD --> CHECK_ATOMIC[Is any KP flagged as atomic?]
        CHECK_ATOMIC -->|no| ADD_ALL[Add all installations]
        CHECK_ATOMIC -->|yes| ADD_RANDOM

            ADD_RANDOM[Add a single valid random installation.]
    ```

1. When other inboxes check for missing installations

    ```mermaid
    graph TD
    CHECK_MISSING[Caro checks for missing installations] --> ATOMIC_CHECK[Are any of Bo's leaf nodes flagged at atomic?]
    ATOMIC_CHECK -->|no| ADD_ALL[Add all missing installations.]
    ATOMIC_CHECK -->|yes| PARTICIPATE[Does Bo have >= 1 installation in group?]
    PARTICIPATE -->|yes| SKIP[Bo should manage his own installations.]
    PARTICIPATE -->|no| AT_LEAST_ONE[Bo needs at least one active installation in the group. Add random installation.]
    ```

1. When an atomic inbox checks their own installations

    ```mermaid
    graph TD
        CHECK[Bo checks installations as an atomic inbox]
        TOO_MANY[Do I have too many installations?]
        TOO_FEW[Do I have too few installations?]
        DO_NOTHING[Do nothing]
        RANDOMLY_RETAIN[Retain random installations including myself.]
        RANDOMLY_ADD[Add random installations.]

        CHECK --> TOO_MANY
        CHECK --> TOO_FEW
        
        TOO_MANY -->|yes| RANDOMLY_RETAIN
        TOO_MANY -->|no| DO_NOTHING
        
        TOO_FEW -->|yes| RANDOMLY_ADD
        TOO_FEW -->|no| DO_NOTHING

    ```

    Effectively, if your inbox is flagged as atomic, others will ensure at least one of your active installations is in the group. Otherwise you manage your own installations in the groups you are a participant of. This is done to keep other members from having to frequently download your key-packages to check if you’re still atomic/balanced.

1. Bo (an atomic inbox) joins a new group

    When an atomic installation joins a new group, if the number of Bo’s installations in that group exceed the configured installation limit for that client, that installation will immediately remove other installations until the limit is satisfied. This allows for other “over-capacity” installations to effectively remove themselves from groups by adding other installations that are “under-capacity”. This can only be done by adding extra installations at limit+1 to prevent race conditions with removal.

### New sync group message type: `InstallationLimit`

A new sync group message type will be present. This will set the installation limit for all installations. The last installation limit message in the sync group will be the value for all installations.

```protobuf
message InstallationLimit {
  // set to None to disable atomic inbox
    optional int32 installation_limit = 1;
    // installation_ids in this list will not be added
    // to new groups.
    repeated bytes frozen_installations = 2;
}
```

### New client functions

1. `client.enable_atomic(limit: usize)`: This will send a sync group message to all installations, signaling all installations to create atomic KPs.
2. `client.disable_atomic()`: This will send a sync group message signaling all installations to disable atomic KPs.
3. `client.is_atomic()`: Returns true or false.

### What happens if I enable this feature and I already have multiple installations?

An over-limit check will be made by the atomic inbox during the missing installations check. If it sees that it has too many installations in the group, the installation will remove other installations until that limit is met.

### What happens when I revoke an installation?

If an installation gets revoked and it was the sole member of a group, other regular members will ensure that at least one valid installation is a member of a group for atomic inboxes, ensuring that you are re-added so long as you still have a valid installation.

### What if a client goes over capacity?

An over-capacity installation can add an installation that is under-capacity. That under-capacity atomic installation will do a check on join to ensure that the limit is met. If the installation number is over the limit, it will remove the over-capacity installation.

## Backward compatibility

This is a breaking change. Old installations that don’t have this extension will ignore the atomic flag and add the missing installations. This will also cause invalid commits on installations without the extension, because they will expect all of the installations to be present.

## Security considerations

This extension limits the number of installations in a group. One attack vector would be to add an inactive installation to the group. However, as mentioned above, other members would notice that installation is inactive, and swap it out.

### Threat model

See [More secure inboxes](#more-secure-inboxes).

## Agent functionality

When this feature is implemented, if you’re an agent developer and want to deploy more than one bot, enabling this feature is relatively simple.

1. Ensure that your installations are pruned.
    1. Be sure to revoke any installations that are no longer online.
2. Call `client.emable_atomic()`

This will flag your inbox as an atomic inbox, and now you will only have one installation per-group. This will allow you to create multiple agents with ease.

## Rationale

### Alternative design: Keep things the way they are, and have each installation stream a subset of groups

The benefit of this is this solution is less complex on the surface, but also comes with some hidden costs.

- It does not reduce commit size at all.
  - If an agent has 10 bots, those 9 bots that do not stream the group will still occupy the leaf nodes of every group they’re not participating in.
  - If that agent is a part of 10k DMs, that’s 9k leaf node slots that are effectively doing nothing. Every DM that would have had a ratchet tree depth of 2 (assuming the other only has 1 installation), now has a depth of 5. Going from 3 nodes to 31.
- Not only does it not reduce commit sizes, it actually increases their size.
  - Inactive installations in groups still need to update their leaf nodes.
  - Stale leaf nodes increase commit sizes, because they need to be encrypted to outside of the current tree.
- There’s the deceptively complex issue of figuring out which installation is going to stream a new group when it arrives.
- When a dev removes an installation, they have to redistribute those groups it was a part of to the other installations. Those installations will have to “catch up”.
  - This will mean that XMTP has to keep commits around forever, which is something that is currently planned, but it would be nice to not design ourselves into that corner.
  - XMTP sync cursors are global per-originator. When we omit groups, and then later un-omit them, that will complicate the sync logic.
- A well-tuned sqlite3 database can handle tens of gigabytes of data, probably more. But it would still be nice to proactively do things that don’t push XMTP toward that limit.

The core of this proposal is effectively a flag in the node that tells other installations not to add missing installations for this inbox and that the atomic inbox will manage itself. By reducing the number of installations, we reduce the size of group ratchet trees, prevent stale nodes, and don’t have to worry about managing group_ids for streaming subsets of the group pool by leveraging existing membership logic to keep things as simple as possible.

The main benefit of streaming subsets of groups is it keeps something specific to agents out of the way of the rest of the protocol. But I believe there are also uses of atomic membership outside of agents.

### More secure inboxes

Having a flag that says “Nah, I’m good. Let me manage my own installations.” has a great security benefit for the extra paranoid. For example, if your computer gets hacked and you lose your root signing key, traditionally that attacker would be able to use that root signing key to add a new installation and other members (including the installation on your phone) would add that attacker’s new installation to all of your groups.

But if your inbox is flagged as atomic, this would allow you to build a mechanism where you would have to approve adding new installations to groups. You’d get a prompt to add a new installation that you’re not aware of, which of course - you’d deny. And now the attacker is left without the secret keys to your super secure group.

You would still need to get a new root signing key and consequentially a new inbox, but now you can be sure that the bad actor will not gain access to your very private group, and all you need to do is create a new inbox, and add two proposals to the group:

1. Add new inbox.
2. Remove self.

However, there’s still the attack vector of an installation adding their own installation, then immediately revoking all of yours that needs to be solved for, but I do think that could be solved for in an upcoming XIP. Probably some form of 2FA with a second signing key.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
