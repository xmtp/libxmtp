# @xmtp/mls-client-bindings-node

## 0.0.11

- Added `inbox_state` to client
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
- Added `find_inbox_id_by_address` to client
- Added `find_group_by_id` and `find_message_by_id` to conversations

## 0.0.6

- Fixed some group syncing issues

## 0.0.5

- Added ability to set group name and image URL during creation
- Added getter and setter for group image URL
- Renamed `add_erc1271_signature` to `add_scw_signature`

## 0.0.4

- Added `stream_all_messages`

## 0.0.3

- Fixed default export value

## 0.0.2

- Added inbox ID helpers
- Refactored identity strategy creation
- Added permissions functions to groups

## 0.0.1

Initial release
