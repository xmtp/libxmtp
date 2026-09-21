---
xip: 72
title: Client enforcement of protocol version on specific commits and groups
description: Tool for protocol developers to protect users on old versions from getting groups in a bad state when releasing breaking changes
author: Cameron Voell (@cameronvoell)
status: Final
type: Standards
category: XRC
created: 2025-03-05
---

## Abstract

In XMTP Protocol, breaking changes can sometimes cause errors in existing groups that are difficult or impossible to recover from without just starting over from scratch with a new group. Two ways around this are blocking users on old versions from using the network (API Enforcement, or “The Hammer”), and Group Enforcement using MLS [required capabilities](https://www.rfc-editor.org/rfc/rfc9420.html#section-11.1) extension and [Group Reinitialization](https://www.rfc-editor.org/rfc/rfc9420.html#name-reinitialization). This proposal is for a third type of enforcement, Client Enforcement, where older clients will pause individual groups when they receive a commit message that specifies a min LibXMTP version that is higher than their current version.

## Specification

For the purposes of this proposal, the LibXMTP GitHub repo MLS crate cargo package version will serve as the source of truth of each client version.

### How to communicate `minimum_supported_protocol_version`

The first implementation detail we need is a way of communicating via a message what the `minimum_supported_protocol_version` is for properly processing that message. Ideally we can send this as part of the encrypted payload of a message so that we are not leaking information about the types of messages used in a conversation.

Application messages in LibXMTP all have an `EncodedContent` wrapper for which it would be trivial to add a new `minimum_supported_protocol_version` variable on to. However, Commit Messages in XMTP currently are sent encrypted using OpenMLS Commit struct types without any available encrypted wrapper where we could add a new `minimum_supported_protocol_version` field. Even if we were to add a encrypted around MLS commit messages, there remains a problem on notifying users if a group contains messages with `minimum_supported_protocol_version`, when those users join after some messages are already sent.

The proposed solution is to add a new `minimum_supported_protocol_version` mutable metadata field into the MLS Group Context. With this in place we have a few different options on how XMTP messages can set the `minimum_supported_protocol_version` for a group before using a new breaking feature:

1. Existing Add/Remove Commit Messages could contain an additional Group Context Extension proposal to update the `minimum_supported_protocol_version` metadata field.
2. For new types of Commit Messages or Application Messages that we want to prohibit on older versions, we could require that `minimum_supported_protocol_version` be set in order for the message to be sent and/or validated.

One other advantage of having `minimum_supported_protocol_version` located in mutable metadata is that it helps prohibit random users in large groups from setting `minimum_supported_protocol_version` to a arbitrary high version number and pausing the group for the rest of the members, because we could set the permission requirement for setting a `minimum_supported_protocol_version` as a super admin-only action using group permissions.

### Pausing a group when `minimum_supported_protocol_version` requirement not met

We do not need to consider the processing of own messages, since we can prohibit sending a message with a higher `minimum_supported_protocol_version` that your current client version.

So the only places we need to check `minimum_supported_protocol_version` is when validating external messages and when joining new groups.

The first step is to add a new `paused_for_min_version` field to our SQLite `groups` table. Then when we process an external commit message, we can first check to see if the Group Context Mutable Metadata in the commit has a `minimum_supported_protocol_version` set, if it does, we can check it against our clients current version. If we meet the version, we process as normal. If we do not meet the min version, we take the following steps:

1. In the groups table we set the `paused_for_min_version` field to `minimum_supported_protocol_version` for the relevant conversation.
2. We rollback any cursor updates so that the when the group is unpaused after updating, we can re-process the message with the higher `minimum_supported_protocol_version`.

Both `process_own_message` and `process_external_message` will have new checks at the beginning to make sure that the relevant conversation is not paused before processing. If it is paused, we can check the current `CARGO_PKG_VERSION` to see if we now meet the `minimum_supported_protocol_version` . If we are, we clear the `paused_for_min_version` field of our conversation and then proceed processing as normal.

### Handling `minimum_supported_protocol_version` on Welcome Messages

We can allow users to process welcomes as normal, however, there will be one additional step after a welcome is processed and before a group is stored in the local db. At that time we will check the mutable metadata of the group context in the welcome to see if the is a `minimum_supported_protocol_version` set for the group, and if so, set our group to paused if that min version is higher than our current version. Welcome cursor can go on to other groups, since we have the group saved. Syncing the group will not process any messages since it is in paused state, unless the current version is updated to meet the `minimum_supported_protocol_version` requirement.

## Rationale

The main alternative considered for this implementation is using MLS [required capabilities](https://www.rfc-editor.org/rfc/rfc9420.html#section-11.1) extension. When required capabilities are set on a group, new members will not be able to join the group unless their leaf node has the correct capabilities set on it. Group admins will not be able to update the required capabilities of a group unless all current members’ leaf nodes meet the capabilities requirements. Since having a single leaf node without meeting the requirements in the groups blocks them from using new features, the recommended path forward would be to do a [group re-init](https://www.rfc-editor.org/rfc/rfc9420.html#name-reinitialization) with the same members if you want to update the required capabilities without being blocked by an existing leaf node in the group on an old version. The downside of the re-init is that you need some app logic for “stitching” together the old group with the new group, and there may be edge cases that are hard to understand around whether the old group remains active or not.

Advantages of using our existing Mutable Metadata for storing the `minimum_supported_protocol_version`  in this Client Enforcement proposal:

1. We can configure that only admins or super_admins can update the `minimum_supported_protocol_version` for a group.
2. `minimum_supported_protocol_version` can be updated regardless of versions of the rest of the group as opposed to being blocked until all clients update.
3. We don’t have to deal with the complexity of group reinits and stitching together group history and coordinating clients on older version being notified to join the reinit group.
4. All users on the latest version can use the newest features in their groups, regardless if others are slow to update.

## Backward compatibility

Before Client enforcement code is added the new `minimum_supported_protocol_version` will be ignored on older clients. Getting in this capability into an early stable version is essential for ensuring that the enforcement can protect users from getting their groups in a bad state when features are guarded with Client enforcement in the future.

## Test cases

1. Clients should be able to set `minimum_supported_protocol_version` in a group to pause group updates for clients on versions lower than that min version. Paused clients should be able to unpause after the client is updated.

   See [https://github.com/xmtp/libxmtp/blob/cv/pause_groups_when_min_libxmtp_version_exceeds_current/xmtp_mls/src/groups/mod.rs#L5045](https://github.com/xmtp/libxmtp/blob/cv/pause_groups_when_min_libxmtp_version_exceeds_current/xmtp_mls/src/groups/mod.rs#L5045)

2. Clients on old versions should be able to join groups with `minimum_supported_protocol_version` higher than their current version, but those groups should be immediately set to a paused state so that the clients on insufficient versions do not process messages. If a client then updates their client, message processing should receive on subsequent sync requests.

   See [https://github.com/xmtp/libxmtp/blob/cv/pause_groups_when_min_libxmtp_version_exceeds_current/xmtp_mls/src/groups/mod.rs#L5114](https://github.com/xmtp/libxmtp/blob/cv/pause_groups_when_min_libxmtp_version_exceeds_current/xmtp_mls/src/groups/mod.rs#L5114)

3. Clients in paused groups should be alerted when trying to send a message in a paused group that their message can not be sent because their client is at an insufficient version for communicating in the group. Updating and syncing the group should allow them to resume sending messages.

4. In cases where the are several clients all on different versions of LibXMTP, they should all be able to participate in groups without any pausing unless a permissioned user takes an explicit action to set the `minimum_supported_protocol_version` for that group. By default all groups have no `minimum_supported_protocol_version`  set.

## Reference implementation

See this pull request to the LibXMTP repo: [https://github.com/xmtp/libxmtp/pull/1708](https://github.com/xmtp/libxmtp/pull/1708)

## Security considerations

Since a `minimum_supported_protocol_version` set on a group will prohibit all users on lower versions of LibXMTP from participating in the group, we want to make sure that this is only set in targeted cases, where not setting the `minimum_supported_protocol_version` would lead to irrecoverable errors, or groups that otherwise would not be able to function properly.

### Threat model

In large groups we would want to protect against any member being able to set arbitrarily high `minimum_supported_protocol_version` at any time and pausing the group for all other members. The main mechanism for preventing this would be by default only letting group super admins update `minimum_supported_protocol_version` in a group.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
