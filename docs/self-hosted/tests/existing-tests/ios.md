# iOS SDK test index

[← Test inventory](../existing-tests.md) · [Requirements](../existing-requirements.md)

| File | Qualified test | Form / gates / cases | Requirements |
| --- | --- | --- | --- |
| sdks/ios/Tests/XMTPTests/ArchiveTests.swift | XMTPTests.ArchiveTests.testClientArchives | iOS 15+ | `SHARED-SYNC-REQ-002` |
| sdks/ios/Tests/XMTPTests/ArchiveTests.swift | XMTPTests.ArchiveTests.testInActiveDmsStitchIfDuplicated | iOS 15+ | `SHARED-SYNC-REQ-003` |
| sdks/ios/Tests/XMTPTests/ArchiveTests.swift | XMTPTests.ArchiveTests.testImportArchiveWorksEvenOnFullDatabase | iOS 15+ | `SHARED-SYNC-REQ-004` |
| sdks/ios/Tests/XMTPTests/AttachmentTests.swift | XMTPTests.AttachmentsTests.testCanUseAttachmentCodec | iOS 15+ | `IOS-REQ-004` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testTakesAWallet | iOS 15+ | `SHARED-IDENTITY-REQ-009` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testPassingEncryptionKey | iOS 15+ | `SHARED-IDENTITY-REQ-009` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testStaticCanMessage | iOS 15+; loop over 3 identity results | `SHARED-IDENTITY-REQ-001` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testStaticInboxState | iOS 15+ | `SHARED-IDENTITY-REQ-002` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanDeleteDatabase | iOS 15+ | `SHARED-IDENTITY-REQ-011` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanDropReconnectDatabase | iOS 15+ | `SHARED-IDENTITY-REQ-011` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanMessage | iOS 15+; registered and unregistered | `SHARED-IDENTITY-REQ-001` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testPreAuthenticateToInboxCallback | iOS 15+; 30 s expectation | `SHARED-IDENTITY-REQ-013` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testPassingEncryptionKeyAndDatabaseDirectory | iOS 15+; directory present and nil | `IOS-REQ-011` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testEncryptionKeyCanDecryptCorrectly | iOS 15+; correct and wrong key | `SHARED-IDENTITY-REQ-012` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanGetAnInboxIdFromAddress | iOS 15+ | `SHARED-IDENTITY-REQ-001` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCreatesAClient | iOS 15+ | `SHARED-IDENTITY-REQ-009` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testRevokeInstallations | iOS 15+; 3 to 2 installations | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testRevokesAllOtherInstallations | iOS 15+; 3 to 1 installations | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testsCanFindOthersInboxStates | iOS 15+; 2 inboxes | `SHARED-IDENTITY-REQ-002` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testAddAccounts | iOS 15+; 2 added wallets | `SHARED-IDENTITY-REQ-003` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testAddAccountsWithExistingInboxIds | iOS 15+; default and reassign | `SHARED-IDENTITY-REQ-003` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testRemovingAccounts | iOS 15+; ordinary and recovery removal | `SHARED-IDENTITY-REQ-003` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testSignatures | iOS 15+; correct and wrong message or key; recreated client | `SHARED-IDENTITY-REQ-005` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCreatesAClientManually | iOS 15+; deprecated manual FFI | `IOS-REQ-021` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanManageAddRemoveManually | iOS 15+; deprecated manual FFI | `SHARED-IDENTITY-REQ-003` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanManageRevokeManually | iOS 15+; deprecated manual FFI; selected and all-other | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testPersistentLogging | iOS 15+; filesystem loop over log files | `IOS-REQ-024` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testNetworkDebugInformation | iOS 15+; exact counter snapshot | `IOS-REQ-025` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanSeeKeyPackageStatus | iOS 15+; iterates only returned map keys; empty map passes; lifetime duration check is conditional | `SHARED-IDENTITY-REQ-008` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCanBeBuiltOffline | iOS 15+ | `SHARED-IDENTITY-REQ-010` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testCannotCreateMoreThan5Installations | iOS 15+; loop creates 10; boundary 11 | `SHARED-IDENTITY-REQ-007` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testStaticRevokeOneOfFiveInstallations | iOS 15+; loop creates 5 | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testStaticRevokeAllInstalltions | iOS 15+; loop creates and revokes 5 | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testStaticRevokeInstallationsManually | iOS 15+; deprecated manual static FFI | `SHARED-IDENTITY-REQ-006` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testGetNewestMessageMetadata | iOS 15+ | `IOS-REQ-031` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testApiClientCacheKeysDifferentConfigurations | iOS 15+; async only; multiple configurations | `IOS-REQ-032` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testClientOptionsDefaultsDbPoolOptionsToNil | iOS 15+; synchronous | `SHARED-IDENTITY-REQ-014` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testClientOptionsCarriesDbPoolOptions | iOS 15+; synchronous | `SHARED-IDENTITY-REQ-014` |
| sdks/ios/Tests/XMTPTests/ClientTests.swift | XMTPTests.ClientTests.testClientOptionsDbPoolOptionsPartialFields | iOS 15+; synchronous | `SHARED-IDENTITY-REQ-014` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testCanRoundTripWithCustomContentType | iOS 15+ | `IOS-REQ-034` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testFallsBackToFallbackContentWhenCannotDecode | iOS 15+ | `IOS-REQ-035` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testShouldPushForTextCodec | iOS 15+ | `IOS-REQ-036` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testShouldPushForReactionCodec | iOS 15+ | `IOS-REQ-036` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testShouldPushForReadReceiptCodec | iOS 15+ | `IOS-REQ-036` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testShouldPushForCustomCodec | iOS 15+ | `IOS-REQ-036` |
| sdks/ios/Tests/XMTPTests/CodecTests.swift | XMTPTests.CodecTests.testMessageVisibilityOptionsToFfi | iOS 15+; async only | `IOS-REQ-037` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanFindConversationByTopic | iOS 16+; group and DM | `IOS-REQ-038` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanListConversations | iOS 16+; group and DM and peer | `SHARED-GROUP-REQ-034` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanListConversationsFiltered | iOS 16+; allowed, denied, union | `SHARED-GROUP-REQ-033` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanSyncAllConversationsFiltered | iOS 16+; 4 consent filters | `IOS-REQ-041` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanListConversationsOrder | iOS 16+ | `SHARED-GROUP-REQ-032` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanStreamConversations | iOS 16+; at least 2 emissions | `SHARED-GROUP-REQ-028` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanStreamAllMessages | iOS 16+; at least 2 emissions | `SHARED-GROUP-REQ-030` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testReturnsAllHMACKeys | iOS 16+; attempts 5 DMs, catches failures, and checks topics only for 0–5 successful creations; zero can pass | `SHARED-GROUP-REQ-042` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testMessagesDontDisappear | iOS 16+; 1 s wait | `SHARED-GROUP-REQ-025` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testStreamsAndMessages | iOS 16+; concurrent loops; 90 events; 30 s | `SHARED-GROUP-REQ-040` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanCreateOptimisticGroup | iOS 16+ | `IOS-REQ-048` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanStreamAllMessagesFilterConsent | iOS 16+; 2 allowed and 2 denied | `SHARED-GROUP-REQ-030` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testReturnsAllTopics | iOS 16+; two installations and duplicate DM | `SHARED-GROUP-REQ-042` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCanListConversationsAndCheckCommitLogForkStatus | iOS 16+; loop over group and DM | `IOS-REQ-051` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testDeleteMessage | iOS 16+; local-only deletion | `IOS-REQ-052` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCountMessages | iOS 16+; group, DM, status, time | `IOS-REQ-053` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testMessagesWithExcludedContentTypes | iOS 16+; reaction exclusion | `IOS-REQ-054` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testCountMessagesWithExcludedContentTypes | iOS 16+; 4 text and 2 reaction | `IOS-REQ-054` |
| sdks/ios/Tests/XMTPTests/ConversationTests.swift | XMTPTests.ConversationTests.testStreamMessageDeletions | iOS 16+; worker deletion; 3 s wait | `IOS-REQ-055` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testCodec | no gate; synchronous throws | `IOS-REQ-056` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testDecryptingKnownCypherText | no gate; synchronous throws; fixed vector | `IOS-REQ-057` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testGenerateAndValidateHmac | no gate | `IOS-REQ-058` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testGenerateAndValidateHmacWithExportedKey | no gate | `IOS-REQ-058` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testGenerateDifferentHmacKeysWithDifferentInfos | no gate; 2 info values | `IOS-REQ-058` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testValidateHmacWithWrongMessage | no gate | `IOS-REQ-058` |
| sdks/ios/Tests/XMTPTests/CryptoTests.swift | XMTPTests.CryptoTests.testValidateHmacWithWrongKey | no gate | `IOS-REQ-058` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testCanEncodeAndDecodeDeleteMessage | iOS 16+ | `IOS-REQ-062` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageCodecFallback | iOS 16+; synchronous throws | `IOS-REQ-063` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageCodecShouldPush | iOS 16+; synchronous throws | `IOS-REQ-063` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageCodecContentType | iOS 16+; synchronous | `IOS-REQ-063` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testContentTypeDeleteMessageRequestValues | iOS 16+; synchronous | `IOS-REQ-063` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testCanSendAndReceiveDeleteMessage | iOS 16+ | `IOS-REQ-065` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageRequestEquatable | iOS 16+; synchronous | `IOS-REQ-064` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageRequestCodable | iOS 16+; synchronous throws | `IOS-REQ-064` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testReceiverCanDecodeDeleteMessageFromListMessages | iOS 16+; receiver raw list | `IOS-REQ-065` |
| sdks/ios/Tests/XMTPTests/DeleteMessageCodecTests.swift | XMTPTests.DeleteMessageCodecTests.testDeleteMessageContentTypeInListMessages | iOS 16+; sender raw list | `IOS-REQ-065` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testSenderCanDeleteOwnMessageInGroup | iOS 16+ | `IOS-REQ-066` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testDeletedMessageSyncsToOtherClients | iOS 16+ | `IOS-REQ-067` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testAdminCanDeleteOtherUsersMessageInGroup | iOS 16+ | `IOS-REQ-068` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testSenderCanDeleteOwnMessageInDM | iOS 16+ | `IOS-REQ-066` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testDeletedMessageSyncsToOtherClientInDM | iOS 16+ | `IOS-REQ-067` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testDeleteMessageViaConversationWrapper | iOS 16+ | `IOS-REQ-069` |
| sdks/ios/Tests/XMTPTests/DeleteMessageTests.swift | XMTPTests.DeleteMessageTests.testStreamingDeletedMessages | iOS 16+; raw stream; 5 s | `IOS-REQ-070` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanFindDmByInboxId | iOS 16+; existing and absent peers | `SHARED-GROUP-REQ-003` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanFindDmByAddress | iOS 16+; identity lookup | `SHARED-GROUP-REQ-003` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanCreateADm | iOS 16+; opposite callers | `SHARED-GROUP-REQ-001` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanCreateADmWithIdentity | iOS 16+; identity APIs | `SHARED-GROUP-REQ-001` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanListDmMembers | iOS 16+ | `SHARED-GROUP-REQ-006` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCannotStartDmWithSelf | iOS 16+ | `SHARED-GROUP-REQ-005` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCannotStartDmWithAddressWhenExpectingInboxId | iOS 16+; typed error and input | `SHARED-GROUP-REQ-005` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCannotStartDmWithNonRegisteredIdentity | iOS 16+ | `SHARED-GROUP-REQ-005` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testDmStartsWithAllowedState | iOS 16+ | `SHARED-GROUP-REQ-021` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanListDmsFiltered | iOS 16+; allowed, denied, union | `SHARED-GROUP-REQ-033` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanListConversationsOrder | iOS 16+ | `SHARED-GROUP-REQ-032` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanSendMessageToDm | iOS 16+ | `SHARED-GROUP-REQ-026` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanStreamDmMessages | iOS 16+; 3 s | `SHARED-GROUP-REQ-029` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanStreamDms | iOS 16+; one group and one DM | `SHARED-GROUP-REQ-028` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanStreamAllDmMessages | iOS 16+; 2 DMs and events | `SHARED-GROUP-REQ-030` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testDmConsent | iOS 16+; allowed, denied, allowed | `SHARED-GROUP-REQ-021` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testDmDisappearingMessages | iOS 16+; 5 s retention; nil and re-enable; waits | `SHARED-GROUP-REQ-025` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testCanSuccessfullyThreadDms | iOS 16+; independent duplicate DMs | `SHARED-GROUP-REQ-002` |
| sdks/ios/Tests/XMTPTests/DmTests.swift | XMTPTests.DmTests.testLastReadTimes | iOS 16+; one receipt | `SHARED-CONTENT-REQ-012` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testFindMessagesV2ComparedToFindMessages | iOS 16+; raw, enriched, legacy reaction | `IOS-REQ-086` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testBasicMessageRetrievalInBothConversationTypes | iOS 16+; loop group and DM, 3 texts each | `IOS-REQ-087` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testPaginationParameters | iOS 16+; limit, before, after, directions | `SHARED-CONTENT-REQ-013` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testAllContentTypesAndReactions | iOS 16+; 4 types and 2 reactions | `IOS-REQ-086` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testEdgeCasesAndDeliveryStatus | iOS 16+; all count exceeds published count; published rows checked; prepared ID/status check is conditional on a matching unpublished row | `IOS-REQ-090` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testLargeMessageSetPerformance | iOS 16+; full count at least 30 under 2 seconds; first limited page at most 11; second-page nonempty/disjoint checks only if first page is nonempty | `IOS-REQ-091` |
| sdks/ios/Tests/XMTPTests/EnrichedMessagesTests.swift | XMTPTests.EnrichedMessagesTests.testComplexContentTypes | iOS 16+; nested reply and filename checks are conditional; content length equality runs only when non-nil and positive | `IOS-REQ-092` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testGroupCreatedWithCorrectAdminList | iOS 16+ | `SHARED-GROUP-REQ-017` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testGroupCanUpdateAdminList | iOS 16+; promote and remove with metadata gate | `SHARED-GROUP-REQ-017` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testGroupCanUpdateSuperAdminList | iOS 16+ | `SHARED-GROUP-REQ-017` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testGroupMembersAndPermissionLevel | iOS 16+; 3 role distributions | `SHARED-GROUP-REQ-017` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testCanCommitAfterInvalidPermissionsCommit | iOS 16+ | `SHARED-GROUP-REQ-043` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testCanUpdatePermissions | iOS 16+; description admin to allow | `SHARED-GROUP-REQ-020` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testCanCreateGroupWithCustomPermissions | iOS 16+; inbox-ID constructor | `SHARED-GROUP-REQ-019` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testCanCreateGroupWithInboxIdCustomPermissions | iOS 16+; identity constructor despite name | `SHARED-GROUP-REQ-019` |
| sdks/ios/Tests/XMTPTests/GroupPermissionsTests.swift | XMTPTests.GroupPermissionsTests.testCreateGroupWithInvalidPermissionsFails | iOS 16+ | `SHARED-GROUP-REQ-019` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanCreateAGroupWithDefaultPermissions | iOS 16+; inbox-ID creation; invited member adds, is promoted to admin before removing, and creator re-adds | `SHARED-GROUP-REQ-018` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanCreateAGroupWithIdentityDefaultPermissions | iOS 16+; identity creation; invited member adds, is promoted to admin before removing, and creator re-adds | `SHARED-GROUP-REQ-018` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanCreateAGroupWithAdminPermissions | iOS 16+; creator and member actions | `SHARED-GROUP-REQ-018` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListGroups | iOS 16+ | `SHARED-GROUP-REQ-034` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListGroupsFiltered | iOS 16+; allowed, denied, union | `SHARED-GROUP-REQ-033` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListGroupsOrder | iOS 16+ | `SHARED-GROUP-REQ-032` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListGroupMembers | iOS 16+ | `SHARED-GROUP-REQ-045` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanAddGroupMembers | iOS 16+; inbox ID; transcript | `SHARED-GROUP-REQ-011` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCannotStartGroupOrAddMembersWithAddressWhenExpectingInboxId | iOS 16+; create, add, remove address misuse | `SHARED-GROUP-REQ-012` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanAddGroupMembersByIdentity | iOS 16+; identity; transcript | `SHARED-GROUP-REQ-011` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanRemoveMembers | iOS 16+; inbox ID; transcript | `SHARED-GROUP-REQ-011` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanRemoveMembersByIdentity | iOS 16+; identity; transcript | `SHARED-GROUP-REQ-011` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanMessage | iOS 16+; registered and unregistered | `SHARED-IDENTITY-REQ-001` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testIsActive | iOS 16+; before and after removal | `SHARED-GROUP-REQ-013` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testAddedByAddress | iOS 16+ | `SHARED-GROUP-REQ-010` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanStartEmptyGroup | iOS 16+ | `SHARED-GROUP-REQ-007` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCannotStartGroupWithNonRegisteredIdentity | iOS 16+ | `SHARED-GROUP-REQ-012` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testGroupStartsWithAllowedState | iOS 16+ | `SHARED-GROUP-REQ-021` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanSendMessagesToGroup | iOS 16+; text and control payloads | `SHARED-GROUP-REQ-026` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListGroupMessages | iOS 16+; all, published, unpublished, peer | `SHARED-GROUP-REQ-026` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanStreamGroupMessages | iOS 16+; 3 s | `SHARED-GROUP-REQ-029` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanStreamGroups | iOS 16+; group and DM type filter | `SHARED-GROUP-REQ-028` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testStreamGroupsAndAllMessages | iOS 16+; two streams | `SHARED-GROUP-REQ-031` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanStreamAndUpdateNameWithoutForkingGroup | iOS 16+; 5 streamed events, 4 texts, and stored-count checkpoints 3/5/6; no fork or debug-state assertion | `IOS-REQ-118` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanStreamAllGroupMessages | iOS 16+; group and DM type filter | `SHARED-GROUP-REQ-030` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanUpdateGroupMetadata | iOS 16+; before and after peer sync | `SHARED-GROUP-REQ-009` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanUpdateGroupAppData | iOS 16+; initial and replacement | `SHARED-GROUP-REQ-009` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testGroupConsent | iOS 16+; allowed, denied, allowed | `SHARED-GROUP-REQ-021` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanAllowAndDenyInboxId | iOS 16+; allowed and denied member projection | `SHARED-GROUP-REQ-022` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanFetchGroupById | iOS 16+ | `SHARED-GROUP-REQ-004` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanFetchMessageById | iOS 16+; lookup result discarded | `IOS-REQ-163` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testUnpublishedMessages | iOS 16+; publish, status, consent | `SHARED-CONTENT-REQ-002` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanSyncManyGroupsQuickly | iOS 16+; loop 100 groups; under 10 s | `SHARED-GROUP-REQ-038` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanListManyMembersInParallelQuickly | iOS 16+; loop 100 groups; under 10 s | `IOS-REQ-127` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testGroupDisappearingMessages | iOS 16+; 5 s retention; nil and re-enable; waits | `SHARED-GROUP-REQ-025` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testGroupPausedForVersionReturnsNone | iOS 16+; group and DM | `SHARED-IDENTITY-REQ-018` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testPaginationOfConversationsList | iOS 16+; limit-10 second result discarded; limit-5 loop validates all 15 unique group IDs with a ten-page safety bound | `SHARED-GROUP-REQ-037` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testCanLeaveGroup | iOS 16+; 3 s worker wait | `SHARED-GROUP-REQ-014` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testLeftInboxesPopulatedWhenMemberLeaves | iOS 16+; 3 s worker wait | `SHARED-GROUP-REQ-016` |
| sdks/ios/Tests/XMTPTests/GroupTests.swift | XMTPTests.GroupTests.testLeftInboxesPersistedAfterClientReinitialization | iOS 16+; database drop and build | `SHARED-GROUP-REQ-016` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testSyncConsent | iOS 15+; second-installation consent assertions are inside optional conversation lookup with no failing else branch | `IOS-REQ-133` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testSyncMessages | iOS 15+; always XCTSkip before setup; polling loop unreachable | `IOS-REQ-134` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testSyncDeviceArchive | iOS 15+; always XCTSkip before setup | `IOS-REQ-135` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testStreamConsent | iOS 15+; always XCTSkip before setup | `IOS-REQ-136` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testStreamPrivatePreferences | iOS 15+; always XCTSkip before setup | `IOS-REQ-137` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testDisablingHistoryTransferStillSyncsLocalState | iOS 15+; device-sync option omitted and therefore default-enabled; consent checks are conditional on optional conversation lookup | `IOS-REQ-133` |
| sdks/ios/Tests/XMTPTests/HistorySyncTests.swift | XMTPTests.HistorySyncTests.testDisablingHistoryTransferDoesNotTransfer | iOS 15+; device-sync option omitted and therefore default-enabled; requires group lookup and total count 2 but does not inspect message rows | `IOS-REQ-139` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testCanUseLeaveRequestCodec | iOS 16+; nonempty note | `IOS-REQ-140` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestCodecWithNilNote | iOS 16+; nil note | `IOS-REQ-140` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestCodecFallback | iOS 16+; synchronous throws | `IOS-REQ-141` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestCodecShouldPush | iOS 16+; synchronous throws | `IOS-REQ-141` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestCodecContentType | iOS 16+; synchronous | `IOS-REQ-141` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testCanSendAndReceiveLeaveRequestWithCodec | iOS 16+; nonempty note | `IOS-REQ-143` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testCanSendAndReceiveLeaveRequestWithNilNote | iOS 16+; nil note | `IOS-REQ-143` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestEmptyDataNormalizedToNil | iOS 16+; synchronous; empty, nonempty, nil | `IOS-REQ-140` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestEquatable | iOS 16+; synchronous; same, different, nil | `IOS-REQ-142` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestCodable | iOS 16+; synchronous throws | `IOS-REQ-142` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testContentTypeLeaveRequestValues | iOS 16+; synchronous | `IOS-REQ-141` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestMessageIsDecodedProperly | iOS 16+; 3 s worker wait | `IOS-REQ-144` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestContentTypeIsCorrect | iOS 16+; conditional assertion if type found | `IOS-REQ-144` |
| sdks/ios/Tests/XMTPTests/LeaveRequestTests.swift | XMTPTests.LeaveRequestTests.testLeaveRequestFallbackText | iOS 16+; conditional assertion if message found | `IOS-REQ-144` |
| sdks/ios/Tests/XMTPTests/MultiRemoteAttachmentTest.swift | XMTPTests.MultiRemoteAttachmentTests.testCanEncryptAndDecrypt | iOS 16+ and macOS 13+ | `IOS-REQ-145` |
| sdks/ios/Tests/XMTPTests/MultiRemoteAttachmentTest.swift | XMTPTests.MultiRemoteAttachmentTests.testCanUseMultiRemoteAttachmentCodec | iOS 16+ and macOS 13+; loop over 2 attachments | `IOS-REQ-146` |
| sdks/ios/Tests/XMTPTests/MultiRemoteAttachmentTest.swift | XMTPTests.MultiRemoteAttachmentTests.testFromSetsContentLengthFromCiphertext | iOS 16+ and macOS 13+; synchronous throws | `IOS-REQ-157` |
| sdks/ios/Tests/XMTPTests/ReactionTests.swift | XMTPTests.ReactionTests.testCanDecodeLegacyForm | iOS 15+; canonical and legacy | `SHARED-CONTENT-REQ-008` |
| sdks/ios/Tests/XMTPTests/ReactionTests.swift | XMTPTests.ReactionTests.testCanUseReactionCodec | iOS 15+ | `SHARED-CONTENT-REQ-009` |
| sdks/ios/Tests/XMTPTests/ReactionTests.swift | XMTPTests.ReactionTests.testCanDecodeEmptyForm | iOS 15+; canonical and legacy empty values | `SHARED-CONTENT-REQ-008` |
| sdks/ios/Tests/XMTPTests/ReactionTests.swift | XMTPTests.ReactionTests.testCanUseReactionV2Codec | iOS 15+ | `SHARED-CONTENT-REQ-009` |
| sdks/ios/Tests/XMTPTests/ReactionTests.swift | XMTPTests.ReactionTests.testCanMixReactionTypes | iOS 15+; V1 and V2 | `SHARED-CONTENT-REQ-009` |
| sdks/ios/Tests/XMTPTests/ReadReceiptTests.swift | XMTPTests.ReadReceiptTests.testCanUseReadReceiptCodec | iOS 15+ | `SHARED-CONTENT-REQ-011` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testBasic | iOS 16+ and macOS 13+; no-throw only | `IOS-REQ-154` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testCanUseAttachmentCodec | iOS 16+ and macOS 13+; fake HTTPS file fetcher | `IOS-REQ-154` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testCannotUseNonHTTPSUrl | iOS 16+ and macOS 13+; file URL | `IOS-REQ-155` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testVerifiesContentDigest | iOS 16+ and macOS 13+; tampered payload; 3 s | `IOS-REQ-156` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testEncodeOmitsContentLengthWhenNil | iOS 16+ and macOS 13+; synchronous throws | `IOS-REQ-157` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testEncodeWritesContentLengthWhenPresent | iOS 16+ and macOS 13+; synchronous throws | `IOS-REQ-157` |
| sdks/ios/Tests/XMTPTests/RemoteAttachmentTest.swift | XMTPTests.RemoteAttachmentTests.testInitFromEncryptedSetsContentLength | iOS 16+ and macOS 13+; synchronous throws | `IOS-REQ-157` |
| sdks/ios/Tests/XMTPTests/ReplyTests.swift | XMTPTests.ReplyTests.testCanUseReplyCodec | iOS 15+ | `IOS-REQ-158` |
| sdks/ios/Tests/XMTPTests/StreamLifecycleTests.swift | XMTPTests.StreamLifecycleTests.testCatchUpToLiveColdCatchesPendingGroupAndIsIdempotent | iOS 16+; first and second call | `SHARED-GROUP-REQ-039` |
| sdks/ios/Tests/XMTPTests/StreamLifecycleTests.swift | XMTPTests.StreamLifecycleTests.testManageStreamLifecycleDefaultsOn | iOS 16+; synchronous | `SHARED-IDENTITY-REQ-020` |
| sdks/ios/Tests/XMTPTests/TransactionReferencesTests.swift | XMTPTests.TransactionReferenceTests.testCanUseTransactionReferenceCodec | iOS 15+ | `SHARED-CONTENT-REQ-010` |
| sdks/ios/Tests/XMTPTests/VisibilityConfirmationOptionsTests.swift | XMTPTests.VisibilityConfirmationOptionsTests.testToFfiMapsAllFields | no gate; synchronous | `SHARED-IDENTITY-REQ-016` |
| sdks/ios/Tests/XMTPTests/VisibilityConfirmationOptionsTests.swift | XMTPTests.VisibilityConfirmationOptionsTests.testToFfiDefaultsToAllNil | no gate; synchronous | `SHARED-IDENTITY-REQ-016` |
