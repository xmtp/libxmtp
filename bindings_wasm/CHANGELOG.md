# @xmtp/wasm-bindings

## 0.0.16

- Added `isMessageDisappearingEnabled` method to `Conversation`
- Added `messageDisappearingSettings` method to `Conversation`
- Added `removeMessageDisappearingSettings` method to `Conversation`
- Added `messageDisappearingSettings` method to `Conversation`
- Updated JS names for `MessageDisappearingSettings` fields
- Removed automatic filtering of DM group messages

## 0.0.15

- Added `consent_states`, `include_sync_groups`, and `include_duplicate_dms` to `ListConversationsOptions`
- Added `allowed_states` to `GroupQueryArgs`
- Refactored `MessageDisappearingSettings` struct
- Added `consent_states` options to `sync_all_conversations`
- Added `create_group_by_inbox_ids` method to `Conversations`
- Added `find_or_create_dm_by_inbox_id` method to `Conversations`
- Added `ConversationListItem` struct
- Updated `Conversations.list()` method to return `Vec<ConversationListItem>`
- Fixed invalid key package issues
- Fixed rate limiting issues

## 0.0.14

- Removed group pinned frame URL
- Refactored streaming
- Fixed DB locking issues

## 0.0.13

- Fixed DM group validation across installations

## 0.0.12

- Added `getHmacKeys` to `Conversations`

## 0.0.11

- Added installation ID `bytes` to return value of `inboxState`
- Refactored `list`, `listGroups`, and `listDms` to be synchronous

## 0.0.10

- Add ability to revoke installations from a list of installations

## 0.0.9

- Fixed issue that resulted in a forked group

## 0.0.8

- Added support for custom permission policy sets

## 0.0.7

- Moved `verify_signed_with_public_key` out of `Client`

## 0.0.6

- Added `installation_id_bytes` to `Client`
- Added `sign_with_installation_key`, `verify_signed_with_installation_key`, and `verify_signed_with_public_key` to `Client`

## 0.0.5

- Filtered out group membership messages from DM groups

## 0.0.4

- Added smart contract wallet signature support
- Changed package type to `module`
- Upgraded `diesel-wasm-sqlite`
- Added `sqlite3.wasm` to the package
- Added structured logging support

## 0.0.3

- Updated naming conventions for JS exports

## 0.0.2

- Added sort direction to `WasmListMessagesOptions`
- Added `allowed_states` and `conversation_type` to `WasmListConversationsOptions`
- Added `dm_peer_inbox_id` method to `WasmGroup`
- Added `create_dm`, `find_dm_by_target_inbox_id`, `list_groups`, and `list_dms` methods to `WasmConversations`

## 0.0.1

Initial release
