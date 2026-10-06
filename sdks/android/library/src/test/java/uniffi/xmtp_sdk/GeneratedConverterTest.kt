package uniffi.xmtp_sdk

import org.junit.Assert.assertEquals
import org.junit.Test

// The Kotlin converters for these types are generated, so Rust tests cannot
// see them. The deleted instrumented tests were their only Android check.
// Each value goes through a native buffer and back. Distinct field values make
// a swapped, dropped or wrongly read field fail.
class GeneratedConverterTest {
    private fun <T, R> FfiConverter<T, R>.roundTrip(value: T): T = lift(lower(value))

    private fun <T> FfiConverterRustBuffer<T>.checkEach(values: List<T>) {
        for (value in values) assertEquals(value, roundTrip(value))
    }

    private val messageId = "a1".repeat(32)
    private val inboxId = "b2".repeat(32)
    private val otherInboxId = "c3".repeat(32)

    @Test
    fun listOptionsKeepEachField() {
        val conversations =
            ListConversationsOptions(
                kind = ConversationKind.DM,
                consentStates = listOf(ConsentState.DENIED, ConsentState.UNKNOWN),
                createdAfter = Timestamp(1),
                createdBefore = Timestamp(2),
                lastActivityAfter = Timestamp(3),
                lastActivityBefore = Timestamp(4),
                limit = 5u,
                orderBy = ConversationOrder.LAST_ACTIVITY,
                includeDuplicateDms = true,
            )
        assertEquals(conversations, FfiConverterTypeListConversationsOptions.roundTrip(conversations))

        val messages =
            ListMessagesOptions(
                limit = 6u,
                sentBefore = Timestamp(7),
                sentAfter = Timestamp(8),
                insertedBefore = Timestamp(9),
                insertedAfter = Timestamp(10),
                direction = MessageOrder.ASCENDING,
                sortBy = MessageSortBy.INSERTED_AT,
                deliveryStatus = DeliveryStatus.UNPUBLISHED,
                kind = MessageKind.MEMBERSHIP_CHANGE,
                contentTypes = listOf(ContentTypeId("xmtp.org", "text", 1u, 0u)),
                excludeContentTypes = listOf(ContentTypeId("xmtp.org", "reaction", 2u, 0u)),
                excludeSenderInboxIds = listOf(inboxId),
            )
        assertEquals(messages, FfiConverterTypeListMessagesOptions.roundTrip(messages))
        FfiConverterTypeMessageSortBy.checkEach(MessageSortBy.entries)
    }

    @Test
    fun permissionValuesKeepEachVariant() {
        val policies =
            PermissionPolicySet(
                addMember = PermissionPolicy.ALLOW,
                removeMember = PermissionPolicy.DENY,
                addAdmin = PermissionPolicy.ADMIN,
                removeAdmin = PermissionPolicy.SUPER_ADMIN,
                updateName = PermissionPolicy.DOES_NOT_EXIST,
                updateDescription = PermissionPolicy.OTHER,
                updateImage = PermissionPolicy.DENY,
                updateDisappearing = PermissionPolicy.ADMIN,
                updateAppData = PermissionPolicy.ALLOW,
            )
        FfiConverterTypeGroupPermissionMode.checkEach(
            listOf(GroupPermissionMode.AllMembers, GroupPermissionMode.AdminOnly, GroupPermissionMode.Custom(policies)),
        )
        FfiConverterTypePermissionPolicy.checkEach(PermissionPolicy.entries)
        FfiConverterTypePermissionUpdateKind.checkEach(PermissionUpdateKind.entries)
        FfiConverterTypeMetadataFieldKind.checkEach(MetadataFieldKind.entries)
        FfiConverterTypePermissionLevel.checkEach(PermissionLevel.entries)
        FfiConverterTypeMembershipState.checkEach(MembershipState.entries)
        FfiConverterTypeCommitLogForkStatus.checkEach(CommitLogForkStatus.entries)
    }

    @Test
    fun clientValuesKeepEachField() {
        val workers =
            WorkerOptions(
                defaultIntervalNs = 11uL,
                intervals =
                    WorkerKind.entries.mapIndexed { index, kind ->
                        WorkerInterval(kind, index.toULong() + 20uL, index.toULong() + 40uL, index % 2 == 0)
                    } + WorkerInterval(WorkerKind.COMMIT_LOG, null, null, null),
            )
        assertEquals(workers, FfiConverterTypeWorkerOptions.roundTrip(workers))

        val api = ApiStats(publish = 1uL, query = 2uL, queryNewest = 3uL, subscribe = 4uL, subscribeStatic = 5uL)
        assertEquals(api, FfiConverterTypeApiStats.roundTrip(api))
        val identity = IdentityStats(getInboxIds = 6uL, verifySmartContractWalletSignatures = 7uL)
        assertEquals(identity, FfiConverterTypeIdentityStats.roundTrip(identity))
    }

    @Test
    fun conversationResultsKeepEachField() {
        val firstKey = HmacKey(ByteArray(32) { (it + 4).toByte() }, 41)
        val secondKey = HmacKey(ByteArray(32) { (it + 5).toByte() }, 42)
        val keys = listOf(firstKey, secondKey)
        assertEquals(keys, FfiConverterSequenceTypeHmacKey.roundTrip(keys))
        val keysByGroup = mapOf("group-a" to keys, "group-b" to listOf(HmacKey(byteArrayOf(6, -1), 43)))
        assertEquals(keysByGroup, FfiConverterMapStringSequenceTypeHmacKey.roundTrip(keysByGroup))

        val summary = GroupSyncSummary(eligible = 12uL, synced = 13uL)
        assertEquals(summary, FfiConverterTypeGroupSyncSummary.roundTrip(summary))

        val statuses =
            mapOf(
                "installation-a" to KeyPackageStatus(KeyPackageLifetime(notBefore = 14uL, notAfter = 15uL), null),
                "installation-b" to KeyPackageStatus(null, "expired"),
            )
        assertEquals(statuses, FfiConverterMapStringTypeKeyPackageStatus.roundTrip(statuses))

        val readTimes = mapOf(inboxId to Timestamp(16), otherInboxId to Timestamp(17))
        assertEquals(readTimes, FfiConverterMapStringTypeTimestamp.roundTrip(readTimes))
        assertEquals(Timestamp(18), FfiConverterTypeTimestamp.roundTrip(Timestamp(18)))
    }

    @Test
    fun conversationStateKeepsEachField() {
        val disappearing = DisappearingSettings(from = Timestamp(19), retentionNs = 20)
        val createGroup =
            CreateGroupOptions(
                permissions = GroupPermissionMode.AdminOnly,
                name = "name",
                imageUrl = "https://example.test/image",
                description = "description",
                disappearing = disappearing,
                appData = "app data",
            )
        assertEquals(createGroup, FfiConverterTypeCreateGroupOptions.roundTrip(createGroup))
        assertEquals(CreateGroupOptions(), FfiConverterTypeCreateGroupOptions.roundTrip(CreateGroupOptions()))
        FfiConverterTypeCreateDmOptions.checkEach(listOf(CreateDmOptions(disappearing), CreateDmOptions(null)))

        val common =
            ConversationState(
                isActive = true,
                consentState = ConsentState.DENIED,
                pausedForVersion = "1.2.3",
                isDisappearingEnabled = true,
                disappearingSettings = disappearing,
                notificationsEnabled = false,
                commitLogForkStatus = CommitLogForkStatus.FORKED,
            )
        // Each pair of Boolean fields differs in one of the two values, so a swap fails.
        val cleared = common.copy(pausedForVersion = null, isDisappearingEnabled = false, disappearingSettings = null)
        FfiConverterTypeConversationState.checkEach(listOf(common, cleared))

        val group =
            GroupState(
                common = common,
                name = "name",
                imageUrl = "https://example.test/image",
                description = "description",
                appData = "app data",
                membershipState = MembershipState.PENDING_REMOVE,
                admins = listOf(inboxId),
                superAdmins = listOf(otherInboxId),
                permissions =
                    GroupPermissions(
                        GroupPolicyType.CUSTOM,
                        PermissionPolicySet(
                            addMember = PermissionPolicy.ALLOW,
                            removeMember = PermissionPolicy.DENY,
                            addAdmin = PermissionPolicy.ADMIN,
                            removeAdmin = PermissionPolicy.SUPER_ADMIN,
                            updateName = PermissionPolicy.DOES_NOT_EXIST,
                            updateDescription = PermissionPolicy.OTHER,
                            updateImage = PermissionPolicy.DENY,
                            updateDisappearing = PermissionPolicy.ADMIN,
                            updateAppData = PermissionPolicy.ALLOW,
                        ),
                    ),
            )
        FfiConverterTypeGroupState.checkEach(listOf(group, group.copy(common = cleared)))
    }

    @Test
    fun messageContentKeepsEachStandardVariant() {
        val remote =
            RemoteAttachment(
                "https://example.test/file",
                "digest",
                ByteArray(32) { 1 },
                ByteArray(32) { 2 },
                ByteArray(12) { 3 },
                "https",
                10u,
                "file",
            )
        FfiConverterTypeMessageContent.checkEach(
            listOf(
                MessageContent.ReadReceipt,
                MessageContent.Reaction(
                    messageId,
                    inboxId,
                    Reaction("U+1F603", ReactionAction.REMOVED, ReactionSchema.UNICODE),
                ),
                MessageContent.Reaction(
                    messageId,
                    null,
                    Reaction("smile", ReactionAction.ADDED, ReactionSchema.SHORTCODE),
                ),
                MessageContent.Attachment(Attachment("test.txt", "text/plain", byteArrayOf(0, 1, -1))),
                MessageContent.Attachment(Attachment(null, "image/png", byteArrayOf())),
                MessageContent.MultiRemoteAttachment(
                    MultiRemoteAttachment(listOf(remote, remote.copy(filename = null))),
                ),
                MessageContent.TransactionReference(
                    TransactionReference(
                        "eip155",
                        "1",
                        "0xabc",
                        TransactionMetadata("transfer", "ETH", 0.05, 18u, "0xAlice", "0xBob"),
                    ),
                ),
                MessageContent.TransactionReference(TransactionReference(null, "2", "0xdef", null)),
                MessageContent.LeaveRequest(LeaveRequest("note".toByteArray())),
                MessageContent.LeaveRequest(LeaveRequest(null)),
                MessageContent.Reply(messageId, MessageBody.Text("reply body")),
                MessageContent.DeletedMessage(DeletedMessage(DeletedBy.Sender)),
                MessageContent.DeletedMessage(DeletedMessage(DeletedBy.Admin(otherInboxId))),
            ),
        )
    }
}
