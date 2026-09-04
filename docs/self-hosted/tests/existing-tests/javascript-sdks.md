# Browser and Node JavaScript SDK test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

- Inventory: 423 source declarations in 30 test-bearing files. Browser has 224 declarations in 15 files. Node has 199 declarations in 15 files.
- Browser runner: Vitest browser mode uses headless Playwright with one Chromium instance and a 120-second test timeout. Vite serves the repository root so it can load the portal-linked WASM binding. Browser client tests use module Workers and OPFS-backed databases.
- Node runner: Vitest uses globals, a 120-second test timeout, a 60-second hook timeout, and XMTP_NO_PANIC_ON_DB_LOCK=true. Global teardown removes database files.
- CI starts the local backend first. Browser CI uses four Vitest shards. Node CI uses two shards. Local execution must build and stage the matching WASM or Node binding first.
- Declaration forms: all 423 rows use it. No source declaration uses .each, .todo, or .only. Data matrices and asynchronous consumption loops stay inside the listed source declaration.
- Gate: the 12 OPFS declarations inherit describe.skip. Their two nested groups inherit describe.sequential and share database-path state. All other declarations are active.
- Type and documentation checks: expectTypeOf assertions in content-type and validHex tests are included in their containing rows. README examples and helper or fixture declarations do not execute and are not index rows.
- Ambiguities: the browser archive-list check catches every error, so that row does not require browser archive listing to succeed. Several Node createBackend option tests only assert successful construction. Group pausedForVersion creates a DM. The skipped OPFS cases depend on order and shared state.
- Timing conditions: integration tests use the local XMTP backend, distinct database paths, fixed stream-settle delays, fake timers, or worker polling. Debug counter and preference-stream expectations differ where browser workers can add background activity.
- Platform contract difference: the browser stream default is six retries with a 10,000 ms delay. The Node source default is ten retries with a 60,000 ms delay; the Node tests directly observe the maximum of ten but do not directly assert the delay.

| File | Fully qualified test name | Form, gates, and cases | Requirements |
| --- | --- | --- | --- |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should return values from push() in sequence | it; active; Queue values 1 through 5 and break after value 3. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should handle values added during iteration | it; active; Push value 1, then values 2 and 3 while consuming. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should catch an error thrown in the for..await loop and cleanup properly | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should end for await..of loop when stream is ended and call onDone | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should handle multiple concurrent next() calls | it; active; Three pending reads and three ordered pushes. | `JSDK-REQ-001` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should handle return() with pending promises | it; active; One source-body scenario. | `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should not process callbacks after being done | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | AsyncStream > should handle queue properly when values arrive faster than consumption | it; active; Loop to push five values and read the first three. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should only expose allowed methods and properties | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should prevent setting properties | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly forward next() calls to the underlying stream | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly forward end() calls to the underlying stream | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should maintain async iterator functionality | it; active; One asynchronous iteration scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should end for await..of loop when proxy is ended and call onDone | it; active; One asynchronous iteration scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement has() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement ownKeys() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement getOwnPropertyDescriptor() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should handle concurrent operations through proxy | it; active; Three pending reads and three ordered pushes. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should work correctly when stream is already done | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should create a client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should create a client without a signer | it; active; One source-body scenario. | `JSDK-REQ-027` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should return a version | it; active; One source-body scenario. | `JSDK-REQ-030` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should register an identity | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should be able to message registered identity | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should be able to check if can message without client instance | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should get an inbox ID from an address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should return the correct inbox state | it; active; Own state and remote fetched state. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should add a wallet association to the client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-003` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should revoke a wallet association from the client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-003` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should revoke all other installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should revoke specific installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should throw when trying to create more than 10 installations | it; active; Installations 1 through 11; browser also revokes and creates again. | `SHARED-IDENTITY-REQ-007` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should change the recovery identifier | it; active; One source-body scenario. | `JSDK-REQ-038` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should not fail when revoking all other installations with only one installation | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should statically revoke specific installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should verify signatures | it; active; Correct text signature through client and public-key APIs. | `SHARED-IDENTITY-REQ-005` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should transfer an identifier to a new inbox | it; active; One source-body scenario. | `JSDK-REQ-041` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should get key package statuses for installation ids | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-008` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should get inbox state from inbox ids without a client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should get latest inbox updates count from inbox IDs without a client | it; active; One source-body scenario. | `JSDK-REQ-045` |
| sdks/js/browser-sdk/test/Client.test.ts | Client > should get own inbox updates count from a client | it; active; One source-body scenario. | `JSDK-REQ-045` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-054` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should not have initial conversations | it; active; One source-body scenario. | `JSDK-REQ-055` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should get a group or DM by ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-004` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should get a DM by inbox ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-003` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should get a DM by identifier | it; active; One source-body scenario. | `SHARED-GROUP-REQ-003` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should get a message by ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-044` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should list conversations with options | it; active; Seven type, time, limit, and order queries in one body. | `JSDK-REQ-059` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should stream new conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should only stream group conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should only stream dm conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should stream all messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should only stream group conversation messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should only stream dm messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should sync groups across installations | it; active; One source-body scenario. | `JSDK-REQ-067` |
| sdks/js/browser-sdk/test/Conversations.test.ts | Conversations > should stitch DM groups together | it; active; One source-body scenario. | `SHARED-GROUP-REQ-002` |
| sdks/js/browser-sdk/test/DebugInformation.test.ts | DebugInformation > should return network API statistics | it; active; One source-body scenario. | `JSDK-REQ-126` |
| sdks/js/browser-sdk/test/DeviceSync.test.ts | DeviceSync > should sync consent across installations | it; active; Two installations; repeated toggle and poll for Denied, then Allowed. | `JSDK-REQ-123` |
| sdks/js/browser-sdk/test/DeviceSync.test.ts | DeviceSync > should sync device archive using sendSyncArchive, listAvailableArchives, and processSyncArchive | it; active; Messages and Consent archive; two messages exist before processing; requires at least two after; original-message assertion is conditional on at least three; browser archive-list assertion is inside try/catch | `JSDK-REQ-124` |
| sdks/js/browser-sdk/test/DeviceSync.test.ts | DeviceSync > should sync messages across installations using sendSyncRequest and syncAllDeviceSyncGroups | it; active; Messages and Consent request with a 90-second round-trip poll. | `SHARED-SYNC-REQ-007` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-069` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should create a dm | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-001` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should create a DM with identifier | it; active; One source-body scenario. | `JSDK-REQ-072` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should send and list messages | it; active; One source-body scenario. | `SHARED-GROUP-REQ-026` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should optimistically send and list messages | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-002` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should stream messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-029` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should manage consent state | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should handle disappearing messages | it; active; Settings, peer propagation, two expirations, two deletion events, removal metadata, and later persistence. | `SHARED-GROUP-REQ-025` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should return paused for version | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-018` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should get debug info | it; active; Loop over all cursor records. | `JSDK-REQ-080` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should filter messages by content type | it; active; One source-body scenario. | `JSDK-REQ-089` |
| sdks/js/browser-sdk/test/Dm.test.ts | Dm > should count messages with various filters | it; active; Six default, time-window, and content-type count queries. | `JSDK-REQ-082` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-069` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should create a group | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-007` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should create a group with an identifier | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-072` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should optimistically create a group | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-083` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should produce deterministic ids for a caller-set idempotency key | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-003` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should optimistically create a group with members | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-085` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should create a group with options | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-008` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should update group name | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should update group image URL | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should update group description | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should update group app data | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should add and remove members | it; active; One source-body scenario. | `SHARED-GROUP-REQ-011` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should send and list messages | it; active; One source-body scenario. | `SHARED-GROUP-REQ-026` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should optimistically send and list messages | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-002` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should filter messages with options | it; active; Comprehensive content fixture and eleven list-filter queries. | `JSDK-REQ-089` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should stream messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-029` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should add and remove admins | it; active; One source-body scenario. | `SHARED-GROUP-REQ-017` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should add and remove super admins | it; active; One source-body scenario. | `SHARED-GROUP-REQ-017` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should manage consent state | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should handle disappearing messages | it; active; Settings, peer propagation, two expirations, two deletion events, removal metadata, and later persistence. | `SHARED-GROUP-REQ-025` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should return paused for version | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-018` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should get debug info | it; active; Loop over all cursor records. | `JSDK-REQ-080` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should count messages with various filters | it; active; Six default, time-window, and content-type count queries. | `JSDK-REQ-082` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should have pending removal state after requesting removal from the group | it; active; One source-body scenario. | `SHARED-GROUP-REQ-014` |
| sdks/js/browser-sdk/test/Group.test.ts | Group > should remove a member after processing their removal request | it; active; One source-body scenario. | `SHARED-GROUP-REQ-014` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should list files | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should get file count | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should get pool capacity | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should check if file exists | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should return false when deleting a non-existent file | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with no files > should throw an error when exporting a non-existent file | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-127` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should list files and get file count | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should check if file exists | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should delete an existing file | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should export and import an existing database | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should throw an error when importing an invalid database | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Opfs.test.ts | Opfs > with a client database > should clear all files | it; inherited describe.skip; inherited describe.sequential; One source-body scenario. | `JSDK-REQ-128` |
| sdks/js/browser-sdk/test/Preferences.test.ts | Preferences > should return the correct inbox state | it; active; Own state and remote fetched state. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/browser-sdk/test/Preferences.test.ts | Preferences > should get inbox states from inbox IDs | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/browser-sdk/test/Preferences.test.ts | Preferences > should manage consent states | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/browser-sdk/test/Preferences.test.ts | Preferences > should stream consent updates | it; active; Three writes; final write has two consent records in one batch. | `SHARED-SYNC-REQ-008` |
| sdks/js/browser-sdk/test/Preferences.test.ts | Preferences > should stream preferences | it; active; Poll and collect exactly four updates. | `JSDK-REQ-122` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > should send and receive text content | it; active; One source-body scenario. | `JSDK-REQ-106` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > should send and receive markdown content | it; active; One source-body scenario. | `JSDK-REQ-106` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with added action | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with removed action | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with custom schema | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with shortcode schema | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with text content | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with non-text content (attachment) | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with custom content | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Attachment > should send and receive attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-109` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Attachment > should send and receive attachment content without filename | it; active; One attachment shape named by the test. | `JSDK-REQ-109` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should encrypt and decrypt attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-110` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should send and receive remote attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should send and receive remote attachment content without filename | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > should send and receive multi remote attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > should send read receipts and get last read times | it; active; One source-body scenario. | `JSDK-REQ-112` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content without namespace | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content with empty reference | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content with metadata | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content with multiple calls | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content with metadata and capabilities | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should reject when sending wallet send calls content with metadata and missing `description` field | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should reject when sending wallet send calls content with metadata and missing `transactionType` field | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with all styles | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with expiration | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with image URL | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Intent > should send and receive intent | it; active; One plain or metadata intent variant named by the test. | `JSDK-REQ-117` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Intent > should send and receive intent with metadata | it; active; One plain or metadata intent variant named by the test. | `JSDK-REQ-117` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > should send and receive group updated content | it; active; Loop over ten decoded updates, followed by ten exact payload assertions. | `JSDK-REQ-118` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Custom content types > should send and receive custom content | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-119` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Custom content types > should have undefined content when receiving custom content without codec | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-120` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Custom content types > should send and receive custom content using an object literal codec | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-119` |
| sdks/js/browser-sdk/test/contentTypes.test.ts | Content types > Custom content types > should have undefined content when receiving custom content with decode failure | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-120` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with default options | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with production env | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with local env | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with appVersion | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with apiUrl override | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/createBackend.test.ts | createBackend > should create a backend with gateway host | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/browser-sdk/test/inboxId.test.ts | generateInboxId > should generate an inbox id | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/inboxId.test.ts | getInboxIdForIdentifier > should return `undefined` inbox ID for unregistered address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/inboxId.test.ts | getInboxIdForIdentifier > should return inbox ID for registered address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should create a group with default permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-018` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should create a group with admin only permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-018` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should create a group with custom permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-019` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should update group permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce add member policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce remove member policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce add admin policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce remove admin policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update group name policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update group description policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update group image url policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update message disappearing policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update message disappearing policy with allow policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should deny update message disappearing with deny policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/permissions.test.ts | Group permissions > should enforce update app data policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/browser-sdk/test/signer.test.ts | createEOASigner > should create a client with the signer | it; active; One source-body scenario. | `JSDK-REQ-046` |
| sdks/js/browser-sdk/test/signer.test.ts | createEOASigner > should register a client with the signer | it; active; One source-body scenario. | `JSDK-REQ-046` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > basic functionality > should create a stream and emit values | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > basic functionality > should ignore undefined values | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > basic functionality > should call onEnd when stream ends | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > basic functionality > should call streamCloser when stream ends | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > basic functionality > should work with default options | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream value mutators > should apply sync mutator to values | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream value mutators > should apply async mutator to values | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream value mutators > should call onError when sync mutator throws | it; active; One source-body scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream value mutators > should call onError when async mutator rejects | it; active; One source-body scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > error handling > should call onError when stream callback receives an error | it; active; One source-body scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > error handling > should throw StreamInvalidRetryAttemptsError when retryAttempts < 0 and retryOnFail is true | it; active; One retry lifecycle scenario. | `JSDK-REQ-013` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > error handling > should not throw when retryAttempts < 0 and retryOnFail is false | it; active; One retry lifecycle scenario. | `JSDK-REQ-013` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure without retry > should call onFail when stream fails and retryOnFail is false | it; active; One retry lifecycle scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure without retry > should throw StreamFailedError with 0 retries when retryOnFail is false | it; active; One retry lifecycle scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure without retry > should throw StreamFailedError with singular 'time' when retryAttempts is 1 | it; active; One retry lifecycle scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should call onFail when stream fails | it; active; One retry lifecycle scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should call onRetry when retrying | it; active; One retry lifecycle scenario. | `JSDK-REQ-015` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should call onRestart when stream restarts successfully | it; active; One retry lifecycle scenario. | `JSDK-REQ-015` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should fail after max retry attempts with StreamFailedError | it; active; One retry lifecycle scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should use custom retryDelay | it; active; One retry lifecycle scenario. | `JSDK-REQ-013` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > stream failure with retry > should use default retry values | it; active; One retry lifecycle scenario. | `JSDK-REQ-013` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > initial stream function failure > should retry when streamFunction throws initially | it; active; One retry lifecycle scenario. | `JSDK-REQ-015`, `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > initial stream function failure > should throw StreamFailedError when initial failure and retryOnFail is false | it; active; One retry lifecycle scenario. | `JSDK-REQ-014`, `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > retry during retry > should call onFail during retry when stream fails again via onFail callback | it; active; One retry lifecycle scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > retry during retry > should call onEnd and streamCloser after successful retry | it; active; One retry lifecycle scenario. | `JSDK-REQ-010`, `JSDK-REQ-015` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > retry function error handling > should call onRetry with correct attempt numbers when streamFunction throws | it; active; One retry lifecycle scenario. | `JSDK-REQ-015`, `JSDK-REQ-014`, `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > retry function error handling > should call onError for each failed retry attempt | it; active; One retry lifecycle scenario. | `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > edge cases > should handle retryAttempts of 0 | it; active; One retry lifecycle scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > edge cases > should handle no options provided | it; active; One asynchronous iteration scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream > edge cases > should handle no mutator with onValue callback | it; active; One source-body scenario. | `JSDK-REQ-009` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > does not restart after end() during a pending retry | it; active; One retry lifecycle scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > immediately closes a native stream created after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > suppresses onValue and onError after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > does not emit a value whose async mutation resolves after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > allows only one retry in flight per stream | it; active; One retry lifecycle scenario. | `JSDK-REQ-020` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > stays silent when end() precedes the native close callback | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > stays silent when end() precedes the native close callback with retryOnFail disabled | it; active; One retry lifecycle scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > stops retrying after the retry budget is exhausted | it; active; Retry scenario with an internal loop. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > counts failed restart attempts against the retry budget | it; active; One retry lifecycle scenario. | `JSDK-REQ-014`, `JSDK-REQ-012` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > restarts the stream after a failure and continues delivering values | it; active; One retry lifecycle scenario. | `JSDK-REQ-015` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > invokes onEnd once when the stream ends twice | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > ends the stream even when onError throws at terminal failure | it; active; One source-body scenario. | `JSDK-REQ-014` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > suppresses onValue when a sync mutator ends the stream | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > reschedules when the native stream closes during restart creation | it; active; One retry lifecycle scenario. | `JSDK-REQ-020` |
| sdks/js/browser-sdk/test/streams.test.ts | createStream lifecycle > does not create a native stream when onRetry ends the stream | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should return values from push() in sequence | it; active; Queue values 1 through 5 and break after value 3. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should handle values added during iteration | it; active; Push value 1, then values 2 and 3 while consuming. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should catch an error thrown in the for..await loop and cleanup properly | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should end for await..of loop when stream is ended and call onDone | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should handle multiple concurrent next() calls | it; active; Three pending reads and three ordered pushes. | `JSDK-REQ-001` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should handle return() with pending promises | it; active; One source-body scenario. | `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should not process callbacks after being done | it; active; One asynchronous iteration scenario. | `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | AsyncStream > should handle queue properly when values arrive faster than consumption | it; active; Loop to push five values and read the first three. | `JSDK-REQ-001`, `JSDK-REQ-002` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should only expose allowed methods and properties | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should prevent setting properties | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly forward next() calls to the underlying stream | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly forward end() calls to the underlying stream | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should maintain async iterator functionality | it; active; One asynchronous iteration scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should end for await..of loop when proxy is ended and call onDone | it; active; One asynchronous iteration scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement has() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement ownKeys() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should correctly implement getOwnPropertyDescriptor() trap | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should handle concurrent operations through proxy | it; active; Three pending reads and three ordered pushes. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/AsyncStream.test.ts | createAsyncStreamProxy > should work correctly when stream is already done | it; active; One source-body scenario. | `JSDK-REQ-006` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client | it; active; Default client and any options named by the test. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create multiple clients from a single shared backend | it; active; One source-body scenario. | `JSDK-REQ-024` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client with worker config and logging options | it; active; Worker intervals, disabled worker, log, OTLP, resources, and telemetry flush. | `JSDK-REQ-025` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client with DB connection pool options | it; active; Default client and any options named by the test. | `SHARED-IDENTITY-REQ-014` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client with a single DB connection | it; active; Default client and any options named by the test. | `SHARED-IDENTITY-REQ-014` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client without a signer | it; active; Default client and any options named by the test. | `JSDK-REQ-027` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should support a callback function for dbPath client option | it; active; One source-body scenario. | `JSDK-REQ-028` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client with Uint8Array encryption key | it; active; Default client and any options named by the test. | `JSDK-REQ-029` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should create a client with hex string encryption key with 0x prefix | it; active; Default client and any options named by the test. | `JSDK-REQ-029` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should return a version | it; active; One source-body scenario. | `JSDK-REQ-030` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should register an identity | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should be able to message a registered identity | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should be able to check if an identifier can be messaged without a client instance | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should get an inbox ID from an address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should add a wallet association to the client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-003` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should remove a wallet association from the client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-003` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should revoke specific installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should revoke all other installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should not fail when revoking all other installations with only one installation | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should statically revoke specific installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-006` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should throw when trying to create more than 10 installations | it; active; Installations 1 through 11; browser also revokes and creates again. | `SHARED-IDENTITY-REQ-007` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should verify signatures | it; active; Correct text and byte-derived signatures plus one mismatched signature. | `SHARED-IDENTITY-REQ-005` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should check if an address is authorized | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should check if an installation is authorized | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should change the recovery identifier | it; active; One source-body scenario. | `JSDK-REQ-038` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should read key package lifetime for specific installations | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-008` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should throw errors when client is not initialized | it; active; Twenty-five guarded calls and properties. | `JSDK-REQ-043` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should close the client idempotently | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-019` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should get inbox states from inbox IDs without a client | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should get latest inbox updates count from inbox IDs without a client | it; active; One source-body scenario. | `JSDK-REQ-045` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should get own inbox updates count from a client | it; active; One source-body scenario. | `JSDK-REQ-045` |
| sdks/js/node-sdk/test/Client.test.ts | Client > should transfer an identifier to a new inbox | it; active; One source-body scenario. | `JSDK-REQ-041` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-054` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should not have initial conversations | it; active; One source-body scenario. | `JSDK-REQ-055` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should get a group or DM by ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-004` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should get a DM by inbox ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-003` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should get a DM by identifier | it; active; One source-body scenario. | `SHARED-GROUP-REQ-003` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should get a message by ID | it; active; One source-body scenario. | `SHARED-GROUP-REQ-044` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should list conversations with options | it; active; Seven type, time, limit, and order queries in one body. | `JSDK-REQ-059` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should stream new conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should only stream group conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should only stream dm conversations | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-028` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should stream all messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should only stream group conversation messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should only stream dm messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-030` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should sync groups across installations | it; active; One source-body scenario. | `JSDK-REQ-067` |
| sdks/js/node-sdk/test/Conversations.test.ts | Conversations > should stitch DM groups together | it; active; One source-body scenario. | `SHARED-GROUP-REQ-002` |
| sdks/js/node-sdk/test/DebugInformation.test.ts | DebugInformation > should return network API statistics | it; active; One source-body scenario. | `JSDK-REQ-126` |
| sdks/js/node-sdk/test/DeviceSync.test.ts | DeviceSync > should sync consent across installations | it; active; Two installations; repeated toggle and poll for Denied, then Allowed. | `JSDK-REQ-123` |
| sdks/js/node-sdk/test/DeviceSync.test.ts | DeviceSync > should sync device archive using sendSyncArchive, listAvailableArchives, and processSyncArchive | it; active; Messages and Consent archive; two messages exist before processing; requires at least two after; original-message assertion is conditional on at least three; archive list is asserted during retry | `JSDK-REQ-124` |
| sdks/js/node-sdk/test/DeviceSync.test.ts | DeviceSync > should sync messages across installations using sendSyncRequest and syncAllDeviceSyncGroups | it; active; Messages and Consent request with a 90-second round-trip poll. | `SHARED-SYNC-REQ-007` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-069` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should create a dm | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-001` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should create a DM with identifier | it; active; One source-body scenario. | `JSDK-REQ-072` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should send and list messages | it; active; One source-body scenario. | `SHARED-GROUP-REQ-026` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should optimistically send and list messages | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-002` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should stream messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-029` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should manage consent state | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should handle disappearing messages | it; active; Settings, peer propagation, two expirations, two deletion events, removal metadata, and later persistence. | `SHARED-GROUP-REQ-025` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should return paused for version | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-018` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should get debug info | it; active; Loop over all cursor records. | `JSDK-REQ-080` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should filter messages by content type | it; active; One source-body scenario. | `JSDK-REQ-089` |
| sdks/js/node-sdk/test/Dm.test.ts | Dm > should count messages with various filters | it; active; Six default, time-window, and content-type count queries. | `JSDK-REQ-082` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should have a topic | it; active; One source-body scenario. | `JSDK-REQ-069` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should create a group | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-007` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should create a group with an identifier | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-072` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should optimistically create a group | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-083` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should produce deterministic ids for a caller-set idempotency key | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-003` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should optimistically create a group with members | it; active; One creation scenario with the input form named by the test. | `JSDK-REQ-085` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should create a group with options | it; active; One creation scenario with the input form named by the test. | `SHARED-GROUP-REQ-008` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should update group name | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should update group image URL | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should update group description | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should update group app data | it; active; One source-body scenario. | `SHARED-GROUP-REQ-009` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should send and list messages | it; active; One source-body scenario. | `SHARED-GROUP-REQ-026` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should optimistically send and list messages | it; active; One source-body scenario. | `SHARED-CONTENT-REQ-002` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should filter messages with options | it; active; Comprehensive content fixture and eleven list-filter queries. | `JSDK-REQ-089` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should stream messages | it; active; One asynchronous iteration scenario. | `SHARED-GROUP-REQ-029` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should add and remove members | it; active; One source-body scenario. | `SHARED-GROUP-REQ-011` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should add and remove admins | it; active; One source-body scenario. | `SHARED-GROUP-REQ-017` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should add and remove super admins | it; active; One source-body scenario. | `SHARED-GROUP-REQ-017` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should manage consent state | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should handle disappearing messages | it; active; Settings, peer propagation, two expirations, two deletion events, removal metadata, and later persistence. | `SHARED-GROUP-REQ-025` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should return paused for version | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-018` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should get hmac keys | it; active; Loop over returned conversation IDs and three key records. | `SHARED-GROUP-REQ-041` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should get debug info | it; active; Loop over all cursor records. | `JSDK-REQ-080` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should count messages with various filters | it; active; Six default, time-window, and content-type count queries. | `JSDK-REQ-082` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should have pending removal state after requesting removal from the group | it; active; One source-body scenario. | `SHARED-GROUP-REQ-014` |
| sdks/js/node-sdk/test/Group.test.ts | Group > should remove a member after processing their removal request | it; active; One source-body scenario. | `SHARED-GROUP-REQ-014` |
| sdks/js/node-sdk/test/Preferences.test.ts | Preferences > should return the correct inbox state | it; active; Own state and remote fetched state. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/node-sdk/test/Preferences.test.ts | Preferences > should get inbox states from inbox IDs | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-002` |
| sdks/js/node-sdk/test/Preferences.test.ts | Preferences > should manage consent states | it; active; One source-body scenario. | `SHARED-GROUP-REQ-021` |
| sdks/js/node-sdk/test/Preferences.test.ts | Preferences > should stream consent updates | it; active; Three writes; final write has two consent records in one batch. | `SHARED-SYNC-REQ-008` |
| sdks/js/node-sdk/test/Preferences.test.ts | Preferences > should stream preferences | it; active; Fixed sync cadence; collect four updates or end after 10 seconds. | `JSDK-REQ-122` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > should send and receive text content | it; active; One source-body scenario. | `JSDK-REQ-106` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > should send and receive markdown content | it; active; One source-body scenario. | `JSDK-REQ-106` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with added action | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with removed action | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with custom schema | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reaction > should send and receive reaction content with shortcode schema | it; active; One reaction action and schema variant named by the test. | `JSDK-REQ-107` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with text content | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with non-text content (attachment) | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Reply > should send and receive reply with custom content | it; active; One embedded content variant named by the test. | `JSDK-REQ-108` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Attachment > should send and receive attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-109` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Attachment > should send and receive attachment content without filename | it; active; One attachment shape named by the test. | `JSDK-REQ-109` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should encrypt and decrypt attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-110` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should send and receive remote attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > RemoteAttachment > should send and receive remote attachment content without filename | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > should send and receive multi remote attachment content | it; active; One attachment shape named by the test. | `JSDK-REQ-111` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > should send read receipts and get last read times | it; active; One source-body scenario. | `JSDK-REQ-112` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content without namespace | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content with empty reference | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > TransactionReference > should send and receive transaction reference content with metadata | it; active; One namespace, reference, or metadata variant named by the test. | `SHARED-CONTENT-REQ-010` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content with multiple calls | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should send and receive wallet send calls content with metadata and capabilities | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should reject when sending wallet send calls content with metadata and missing `description` field | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > WalletSendCalls > should reject when sending wallet send calls content with metadata and missing `transactionType` field | it; active; One call-count, metadata, capability, or invalid-field variant named by the test. | `SHARED-CONTENT-REQ-014` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with all styles | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with expiration | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Actions > should send and receive actions with image URL | it; active; One style, expiration, image, or base action-set variant named by the test. | `JSDK-REQ-116` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Intent > should send and receive intent | it; active; One plain or metadata intent variant named by the test. | `JSDK-REQ-117` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Intent > should send and receive intent with metadata | it; active; One plain or metadata intent variant named by the test. | `JSDK-REQ-117` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > should send and receive group updated content | it; active; Loop over ten decoded updates, followed by ten exact payload assertions. | `JSDK-REQ-118` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Custom content types > should send and receive custom content | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-119` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Custom content types > should have undefined content when receiving custom content without codec | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-120` |
| sdks/js/node-sdk/test/contentTypes.test.ts | Content types > Custom content types > should have undefined content when receiving custom content with decode failure | it; active; One registered, missing, object-literal, or failing codec variant named by the test. | `JSDK-REQ-120` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with default options | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with a specific env | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with local env | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with gateway host | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with appVersion | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with apiUrl override | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/createBackend.test.ts | createBackend > should create a backend with no optional fields | it; active; One source-body scenario. | `JSDK-REQ-047` |
| sdks/js/node-sdk/test/inboxId.test.ts | generateInboxId > should generate an inbox id | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/inboxId.test.ts | getInboxIdForIdentifier > should return `undefined` inbox ID for unregistered address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/inboxId.test.ts | getInboxIdForIdentifier > should return inbox ID for registered address | it; active; One source-body scenario. | `SHARED-IDENTITY-REQ-001` |
| sdks/js/node-sdk/test/libxmtpErrors.test.ts | LibXMTP errors > should throw when a non-admin tries to add members | it; active; One source-body scenario. | `JSDK-REQ-053` |
| sdks/js/node-sdk/test/libxmtpErrors.test.ts | LibXMTP errors > should throw when adding a non-existent inbox ID | it; active; One source-body scenario. | `JSDK-REQ-053` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should create a group with default permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-018` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should create a group with admin only permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-018` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should create a group with custom permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-019` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should update group permissions | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce add member policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce remove member policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce add admin policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce remove admin policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update group name policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update group description policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update group image url policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update message disappearing policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update message disappearing policy with allow policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should deny update message disappearing with deny policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/permissions.test.ts | Group permissions > should enforce update app data policy | it; active; One multi-step policy and role matrix. | `SHARED-GROUP-REQ-020` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > ends the native stream when the consumer ends the stream | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > does not restart after end() during a pending retry | it; active; One retry lifecycle scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > immediately closes a native stream created after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > suppresses onValue and onError after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > does not emit a value whose async mutation resolves after end() | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > allows only one retry in flight per stream | it; active; One retry lifecycle scenario. | `JSDK-REQ-020` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > stays silent when end() precedes the native close callback | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > stays silent when end() precedes the native close callback with retryOnFail disabled | it; active; One retry lifecycle scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > stops retrying after the retry budget is exhausted | it; active; Retry scenario with an internal loop. | `JSDK-REQ-014` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > counts failed restart attempts against the retry budget | it; active; One retry lifecycle scenario. | `JSDK-REQ-014`, `JSDK-REQ-012` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > restarts the stream after a failure and continues delivering values | it; active; One retry lifecycle scenario. | `JSDK-REQ-015` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > invokes onEnd once when the stream ends | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > ends the stream even when onError throws at terminal failure | it; active; One source-body scenario. | `JSDK-REQ-014` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > suppresses onValue when a sync mutator ends the stream | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > reschedules when the native stream closes during restart creation | it; active; One retry lifecycle scenario. | `JSDK-REQ-020` |
| sdks/js/node-sdk/test/streams.test.ts | createStream lifecycle > does not create a native stream when onRetry ends the stream | it; active; One source-body scenario. | `JSDK-REQ-010` |
| sdks/js/node-sdk/test/streams.test.ts | createStream > should forward StreamFailedError to onError | it; active; One source-body scenario. | `JSDK-REQ-014` |
| sdks/js/node-sdk/test/validation.test.ts | validHex > validates that a string is of type HexString | it; active; Compile-time expectTypeOf narrowing assertion. | `JSDK-REQ-051` |
| sdks/js/node-sdk/test/validation.test.ts | validHex > throws when input is not a valid hex string | it; active; One source-body scenario. | `JSDK-REQ-051` |
| sdks/js/node-sdk/test/validation.test.ts | isHexString > returns true for valid hex strings | it; active; Loop over four valid strings. | `JSDK-REQ-052` |
| sdks/js/node-sdk/test/validation.test.ts | isHexString > returns false for invalid hex strings | it; active; Loop over five invalid strings. | `JSDK-REQ-052` |
| sdks/js/node-sdk/test/validation.test.ts | isHexString > returns false for non-string values | it; active; Loop over number, null, undefined, object, and array. | `JSDK-REQ-052` |
