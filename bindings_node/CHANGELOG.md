# @xmtp/node-bindings

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
- Added `allowed_states` and `conversation_type` to `NapiListConversationsOptions`
- Added `create_dm`, `list_groups`, and `list_dms` methods to `NapiConversations`
- Added `stream_groups`, `stream_dms`, `stream_all_group_messages`, and `stream_all_dm_messages` streaming methods to `NapiConversations`

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
