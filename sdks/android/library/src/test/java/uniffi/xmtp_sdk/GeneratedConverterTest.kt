package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.security.MessageDigest

// The Kotlin converters for these types are generated, so Rust tests cannot
// see them. The deleted Android tests were their only Android check.
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

    @Test
    fun reactionMessagesKeepEachField() {
        val reactions =
            listOf(
                ReactionMessage(
                    id = "d4".repeat(32),
                    senderInboxId = inboxId,
                    sentAt = Timestamp(21),
                    deliveryStatus = DeliveryStatus.FAILED,
                    reaction = Reaction("U+1F44D", ReactionAction.ADDED, ReactionSchema.UNICODE),
                ),
                ReactionMessage(
                    id = "e5".repeat(32),
                    senderInboxId = otherInboxId,
                    sentAt = Timestamp(22),
                    deliveryStatus = DeliveryStatus.UNPUBLISHED,
                    reaction = Reaction("smile", ReactionAction.REMOVED, ReactionSchema.SHORTCODE),
                ),
            )
        assertEquals(reactions, FfiConverterSequenceTypeReactionMessage.roundTrip(reactions))

        // Message.reactions reads this list from the MessageData record.
        val message =
            MessageData(
                id = messageId,
                clientKey = 23uL,
                deliveryCursor = "cursor",
                conversationId = "f6".repeat(16),
                topic = "topic",
                senderInboxId = otherInboxId,
                sentAt = Timestamp(24),
                insertedAt = Timestamp(25),
                expiresAt = Timestamp(26),
                kind = MessageKind.APPLICATION,
                deliveryStatus = DeliveryStatus.PUBLISHED,
                rawBytes = byteArrayOf(7, -7),
                contentType = null,
                fallback = "fallback",
                encoded = null,
                content = MessageContent.Text("text"),
                replyCount = 27uL,
                reactions = reactions,
                inReplyTo = null,
            )
        assertEquals(message, FfiConverterTypeMessageData.roundTrip(message))
    }

    // Live tests reach only a few causes and never a credential kind. The
    // records go through the three generated forms a host reads: the thrown
    // error, the pending status and the bare record. Rust owns which cause,
    // category and retry value a failure gets:
    // xmtp_sdk/src/tests/attachment_flows.rs::attachment_error_category_and_retry_follow_the_cause.
    @Test
    fun attachmentFailuresKeepEachCauseAndCredentialKind() {
        val kinds = CredentialFailureKind.entries
        val failures =
            AttachmentFailureCause.entries.mapIndexed { index, cause ->
                AttachmentFailure(
                    cause = cause,
                    credentialKind = kinds[index % kinds.size],
                    retryable = index % 2 == 0,
                    missingScope = index % 3 != 1,
                    httpStatus = (400 + index).toUShort(),
                )
            } +
                kinds.map { kind ->
                    AttachmentFailure(AttachmentFailureCause.CREDENTIAL, kind, true, true, null)
                } +
                AttachmentFailure(AttachmentFailureCause.NETWORK, null, false, false, null)
        FfiConverterTypeAttachmentFailure.checkEach(failures)
        for (failure in failures) {
            assertEquals(
                PendingAttachmentStatus.Failed(failure),
                FfiConverterTypePendingAttachmentStatus.roundTrip(PendingAttachmentStatus.Failed(failure)),
            )
            val details = ErrorDetails("Attachment", ErrorCategory.CALLBACK, failure.retryable, "failed")
            val thrown = FfiConverterTypeXmtpError.roundTrip(XmtpException.Attachment(details, failure))
            assertTrue("Expected an attachment error, got $thrown", thrown is XmtpException.Attachment)
            assertEquals(details, (thrown as XmtpException.Attachment).v1)
            assertEquals(failure, thrown.v2)
        }
    }

    // Live log tests read only the record target. The deleted check read the
    // fields and the drop count of records that a conformance-only call made.
    @Test
    fun logRecordsKeepEachField() {
        val record =
            LogRecord(
                level = LogLevel.TRACE,
                target = "xmtp_sdk::test",
                message = "message",
                fields = mapOf("sequence" to "0", "inbox" to "b2"),
                timestamp = Timestamp(28),
                droppedRecords = ULong.MAX_VALUE - 2uL,
            )
        assertEquals(record, FfiConverterTypeLogRecord.roundTrip(record))
    }

    // Live tests read only the epoch of a real group.
    @Test
    fun debugInfoKeepsEachField() {
        val info =
            ConversationDebugInfo(
                epoch = ULong.MAX_VALUE - 1uL,
                maybeForked = true,
                forkDetails = "fork",
                isCommitLogForked = false,
                localCommitLog = "local",
                remoteCommitLog = "remote",
                cursor = listOf(ULong.MAX_VALUE, 0uL, 3uL),
            )
        assertEquals(info, FfiConverterTypeConversationDebugInfo.roundTrip(info))
        val unknown = info.copy(maybeForked = false, isCommitLogForked = null, cursor = emptyList())
        assertEquals(unknown, FfiConverterTypeConversationDebugInfo.roundTrip(unknown))
    }

    // The record goes through a native buffer and back first. The native calls
    // then lift keys that Rust made and lower them again, so a wrongly read or
    // written key field makes the decrypt fail.
    @Test
    fun encryptionValuesCrossTheNativeBoundary() =
        runBlocking {
            val keys =
                EncryptionKeys(
                    secret = ByteArray(32) { 8 },
                    salt = ByteArray(32) { 9 },
                    nonce = ByteArray(12) { 10 },
                    digest = "digest",
                    length = 28uL,
                )
            val sealed = EncryptedEncodedContent(byteArrayOf(11, -11), keys)
            assertEquals(sealed, FfiConverterTypeEncryptedEncodedContent.roundTrip(sealed))

            val plaintext = byteArrayOf(5, 6, 7, -1)
            val encrypted = encryptBytes(plaintext)
            assertEquals(
                listOf(32, 32, 12),
                listOf(encrypted.keys.secret.size, encrypted.keys.salt.size, encrypted.keys.nonce.size),
            )
            assertEquals(encrypted.ciphertext.size.toULong(), encrypted.keys.length)
            val digest = MessageDigest.getInstance("SHA-256").digest(encrypted.ciphertext)
            assertEquals(digest.joinToString("") { "%02x".format(it) }, encrypted.keys.digest)
            assertArrayEquals(plaintext, decryptBytes(encrypted.ciphertext, encrypted.keys))

            val content =
                EncodedContent(
                    type = ContentTypeId("xmtp.org", "text", 1u, 0u),
                    parameters = mapOf("encoding" to "UTF-8"),
                    fallback = "fallback",
                    content = "body".toByteArray(),
                )
            val bytes = encodeEncodedContent(content)
            assertArrayEquals(bytes, decryptEncodedContent(encryptEncodedContent(bytes)))
        }
}
