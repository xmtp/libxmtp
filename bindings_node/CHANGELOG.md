# @xmtp/node-bindings

## 1.1.1

- fix: don't delete KeyPackages if processing the welcome messages fails

## 1.0.0

- Improved DM stitching

## 1.0.0-rc3

- Fixed passkey signatures

## 1.0.0-rc2

- Removed an optional `relying_party` field in the `Identifier` struct

## 1.0.0-rc1

- Added `pausedForVersion` to groups for client enforcement
- Removed addresses from all major functions and replaced with new `Identifier`
- Added `addPasskeySignature` as a new signing type

## 0.0.41

- Fix `should_push` field on messages for push notifications

## 0.0.40

- Fixed Rust Panic Error on Streams
- Added `should_push` field on messages for push notifications

## 0.0.39

- Added `content_types` option to `ListMessagesOptions`
- Removed `allowed_states`, `conversation_type`, and `include_sync_groups` from `ListConversationsOptions`
- Added reaction content type
- Added multi remote attachment content type
- Added `find_messages_with_reactions` method to Conversation

## 0.0.38

- Added `version.json` to package
- Added new methods to create groups by inbox ID
- Added consent states option to `sync_all_conversations`
- Updated list conversations options to include `consent_states` and `include_duplicate_dms`
- Removed automatic message filtering from DM groups
- Added disappearing messages methods to conversations
- Updated conversations list methods to return conversations and their last message
- Added consent streaming
- Added preferences streaming

## 0.0.37

- Removed group pinned frame URL
- Fixed DB locking issues

## 0.0.36

- Fixed DM group metadata validation

## 0.0.35

- Updated `createDm` to return an existing DM group, if it exists

## 0.0.34

- Fixed DM group validation across installations

## 0.0.33

- Added installation ID `bytes` to return value of `inboxState`
- Refactored `list`, `listGroups`, and `listDms` to be synchronous

## 0.0.32

- Add ability to revoke installations from a list of installations

## 0.0.31

- Added HMAC keys for push notifications

## 0.0.30

- Fixed issue that resulted in a forked group

## 0.0.29

- Added support for custom permission policy sets

## 0.0.28

- Removed `is_installation_authorized` and `is_address_authorized` from `Client`
- Lowercased `address` passed to `is_address_authorized`

## 0.0.27

- Switched to Ubuntu 22.04 for builds

## 0.0.25

- Fixed streaming by adding `napi4` feature to napi-rs

## 0.0.24

- Fixed using `Vec` instead rust `Uint8Array` type in `is_installation_authorized`

## 0.0.23

- Added `is_installation_authorized` to `Client`
- Added `is_address_authorized` to `Client`

## 0.0.22

- Moved `verify_signed_with_public_key` out of `Client`

## 0.0.21

- Added `installation_id_bytes` to `Client`

## 0.0.20

- Fixed argument types for new signing methods

## 0.0.19

- Renamed `Level` to `LogLevel`
- Filtered out group membership messages from DM groups
- Fixed `syncAllConversations` export
- Added `sign_with_installation_key`, `verify_signed_with_installation_key`, and
  `verify_signed_with_public_key` to `Client`

## 0.0.18

- Added `syncAllConversations` to `Conversations`
- Added smart contract wallet support
- Converted package to ESM

## 0.0.17

- Removed all `Napi` prefixes
- Fixed stream callback argument types
- Renamed `NapiGroup` to `Conversation`

## 0.0.16

- Added sort direction to `NapiListMessagesOptions`
- Added `dm_peer_inbox_id` method to `NapiGroup`
- Added `allowed_states` and `conversation_type` to
  `NapiListConversationsOptions`
- Added `create_dm`, `list_groups`, and `list_dms` methods to
  `NapiConversations`
- Added `stream_groups`, `stream_dms`, `stream_all_group_messages`, and
  `stream_all_dm_messages` streaming methods to `NapiConversations`

## 0.0.15

- Updated to latest `xmtp_mls`

## 0.0.14

- Arguments in stream callback throw error rather than silently ignore

## 0.0.13

- Added logging option when creating a client
- Added `inboxAddresses` to client

## 0.0.12

- Added ability to add wallet associations to a client
- Added ability to revoke wallet associations from a client
- Added ability to revoke all installation IDs from a client
- Added `getLatestInboxState` to client
- Added installation timestamps to `inboxState`
- Updated `send_optimistic` to return the message ID as a hex string
- Added consent state methods to groups and client

## 0.0.11

- Added `inboxState` to client
- Skip duplicate message processing when streaming

## 0.0.10

- Fixed several group syncing issues
- Improved performance

## 0.0.9

- Added optimistic sending
- Added `policySet` to group permissions
- Added pinned frame url to group metadata

## 0.0.8

- Added description option when creating groups
- Added description getter and setter to group instances
- Fixed DB locking issues
- Fixed invalid policy error
- Removed Admin status from group creators (Super Admin only)

## 0.0.7

- Improved streaming welcomes
- Improved DB retries
- Changed encoding of the MLS database to `bincode` for performance
- Added `findInboxIdByAddress` to client
- Added `findGroupById` and `findMessageById` to conversations

## 0.0.6

- Fixed some group syncing issues

## 0.0.5

- Added ability to set group name and image URL during creation
- Added getter and setter for group image URL
- Renamed `addErc1271Signature` to `addScwSignature`

## 0.0.4

- Added `streamAllMessages`

## 0.0.3

- Fixed default export value

## 0.0.2

- Added inbox ID helpers
- Refactored identity strategy creation
- Added permissions functions to groups

## 0.0.1

Initial release
