---
xip: 47
title: Group Chat Permissions
description: This proposal describes a simple, flexible, updatable group chat permissions system.
author: Cameron Voell (@cameronvoell), Eleanor Hofstedt (@eleanorhofstedt)
status: Final
type: Standards
category: Core
created: 2024-05-07
implementation: https://docs.xmtp.org/chat-apps/core-messaging/group-permissions
---

## Abstract

For many compelling group chat use cases—such as those with open invitations, semi-anonymous identities, or a large number of members—a permission system becomes critical for the chat to function effectively. This XIP aims to provide application developers with the tools to implement trustworthy permission features without excessive complexity, while still retaining flexibility.

The following proposal describes how XMTP can use MLS Group Context Extensions to configure which users in a chat are allowed to perform actions like adding or removing group members, as well as who is allowed to modify those permissions over time.

## Specification

### Background

The XMTP Group Chat implementation uses the Messaging Layer Security (MLS) protocol described in [IETF RFC 9420](https://www.rfc-editor.org/rfc/rfc9420.html). The MLS standard includes a `Group Context` object that represents the shared configuration between all group members and contains flexibility for defining custom `UnknownExtensions` that contain arbitrary byte data.

By using MLS Group Context Extensions to store our “Permissions” configuration data, we can ensure that:

- All group members agree on the current state of the permissions parameters of the group
- Settings remain encrypted and unreadable to any non-members
- No one outside the group will have the ability to affect the permissions configuration

Currently, XMTP Group Chat beta provides the following **Permissions Policies**:

- `add_member_policy`
- `remove_member_policy`
- `update_metadata_policy`

Each of these policies can have one of the following **Permission Options**:

- `UNSPECIFIED`
- `ALLOW`
- `DENY`
- `CREATOR_ONLY`

### Mutable Admin Permissions Update Specification

This XIP proposes to add the following new **Permission Policies**:

- `add_admin_policy`
- `remove_admin_policy`
- `update_permissions_policy`

We also propose to add the following new **Permission Options**:

- `ALLOW_IF_ADMIN_OR_SUPER_ADMIN`
- `ALLOW_IF_SUPER_ADMIN`

To enable the two new permission options, we propose adding the following to Mutable metadata Protobuf definitions:

```proto
message GroupMutableMetadataV1 {
...
  AccountAddresses admin_list = 2;
  AccountAddresses super_admin_list = 3;
}

// Wrapper around a list of repeated EVM Account Addresses
message AccountAddresses {
  repeated string account_addresses = 1;
}
```

These new **Permission Options** will enable use cases such as:

- **Admins can update permissions**: The group starts small with open permissions and then adds restrictions over time as they pass a few dozen users so that only admins can remove users or update the group name and project URL.

- **Group creator can add another admin**: The leader of an online project creates a group chat with themself as the super admin who can add and remove admins and update group information, etc. There is no fear that admins will take over the group because the group creator is still the only super admin. After some time, if the group creator wants to move on from the project, they can make someone else the super admin.

- **Admins can update permissions when new functionality is available**: In the future, XMTP may add new group features, such as mute members. These permission options provide an update path so existing groups can get these new features without losing the existing group history.

### Sensible Default Options

This proposal aims to provide only a few default permission sets that can cover most use cases and easily pass in as parameters on group creation. This approach allows flexibility of permissions without making it too confusing for developers and users.

For developers who want to fine-tune individual permission options for different group actions, we will allow those developers to construct their own initial permission set on group creation. In both cases, we will allow updates to permission sets over time, following the `update_permissions_policy`. Example default policies include:

- All Members Policy:  
  - All group members can add new members
  - Only admins can remove members, update metadata
  - Only super admins can add/remove admins, update permissions
- Admins Only Policy:
  - Only admins can add/remove members, update metadata
  - Only super admins can add/remove admins, update permissions

There are a few more rules that apply to all permission policies:

- The group creator always defaults as the group's only super admin
- Admins can not remove super admins from a group
- Only super admins can add other super admins
- Super admins can remove other super admins from the group or remove the super admin role from an address, but there must always be at least one super admin in the group. So removing all super admin will fail. This means you can remove yourself as a super admin if at least one other super admin is in the group.

## Rationale

For anyone who has ever used Discord, Telegram, or tried to maintain the usefulness of a group chat with over a few dozen people, the need for admin functionality in larger-sized groups is self-evident.

One implementation detail we considered is whether permissions could function on a two-tier system of members and admins or if we needed a three-tiered system of members, admins, and super admins.

At this point, contributors agree the three-tier system is necessary based on the use case in which a group creator wants to delegate some responsibilities (`remove_member`, `update_metadata`) to be "admin only," and they don't want to immediately risk other admins removing them as an admin. In other words, the three-tier system allows a happy medium of delegatable admin responsibilities without risking group takeover.

Another consideration was whether the extra complexity is worth making permissions updatable. The choice to allow permissions to be updated, as long as the member performing the update qualifies against the `update_permissions_policy`, seems necessary.

The ability to update permissions is needed to address the use cases of initial group permission misconfiguration and the inevitable evolution of a group’s trust dynamic. For example, just because an online project group chat starts as a small group of well-intentioned contributors does not mean that the group may not evolve to have a larger variety of member trustworthiness and contributor types over time.

## Threat Model

There are three main categories of threats that are considered when designing the permissions system:

1. Can group creators share responsibility with admins while retaining the ability to recover from malicious admins?  
   This threat is mitigated by the three tiered permission system, the fact that super admins can not be removed by admins, and only super admins can update permissions.
2. Does the permissions system have some recoverability in case an action was taken by mistake and needs to be undone?  
   This threat is mitigated by the ability to update permissions, which allows a previously configured permission set to be adjusted if necessary.
3. Is the group permission system safe from malicious actors who are running modified versions of our client code?  
   This threat is mitigated by client side verification of all group actions that are subject to group permission policies.

## Backward compatibility

In addition to adding the new **Permission Policies** and **Permission Options** and making **Permissions Updatable**, we will also make the permission system itself updatable in the following ways:

- `libxmtp` can be updated so existing groups can add new **Permission Policies** via the proto `map<string, MetadataPolicy> update_metadata_policy`. This means that if later on we want to add a new Permission Policy to groups, such as "The Ability to Mute Group Members," we can add it without breaking any existing groups.

- Deprecation and update of the Permissions or Metadata Extensions in an existing group's Group Context

- Existing groups will not be able to include an entirely new Permissions or Metadata Extension until all group members update to a new version of libxmtp and perform a "leaf node update commit" that updates their "supported capabilities."  

  One way this could work for existing groups would be to have a generous grace period, such as a few weeks or months, after which an admin could remove any group member who has not upgraded. We could then update the group to enable new permission / metadata extensions. The hope is that we might never need to perform this type of update or that it will be rare and even less frequent as time passes. For more information, see [MLS RFC - Required Capabilities](https://www.rfc-editor.org/rfc/rfc9420.html#name-required-capabilities).

## Reference implementation

For an example of the updates required for protobuf objects, see this [Mutable Group Permissions PR](https://github.com/xmtp/proto/pull/175) in the XMTP **proto** GitHub repo.

```solidity
// Message for group mutable permissions
message GroupMutablePermissionsV1 {
  PolicySet policies = 1;
}

// The set of policies that govern the group
message PolicySet {
  MembershipPolicy add_member_policy = 1;
  MembershipPolicy remove_member_policy = 2;
  map<string, MetadataPolicy> update_metadata_policy = 3;
  PermissionsUpdatePolicy add_admin_policy = 4;
  PermissionsUpdatePolicy remove_admin_policy = 5;
  PermissionsUpdatePolicy update_permissions_policy = 6;
}

// Message for group mutable metadata
message GroupMutableMetadataV1 {
  // Map to store various metadata attributes (Group name, etc.)
  map<string, string> attributes = 1;  
  AccountAddresses admin_list = 2;
  // Creator starts as only super_admin
  // Only super_admin can add/remove other super_admin
  AccountAddresses super_admin_list = 3;
}

// Wrapper around a list of repeated EVM Account Addresses
message AccountAddresses {
  repeated string account_addresses = 1;
}
```

## Security considerations

The main security considerations for this update are to ensure that:

- Developers and users of XMTP understand how permissions work

- Permissions do not have any bugs that would allow a super admin to be kicked out of their own group or for group permissions to not behave as expected. Though it will likely always be possible for someone to lose admin control over their own group if they take action to remove themself, we will strive to provide tests, documentation, and intuitive default permission options to minimize this risk.

## Copyright

Copyright and related rights waived via [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
