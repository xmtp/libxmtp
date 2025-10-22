package org.xmtp.android.library

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.withContext
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.ConversationDebugInfo
import org.xmtp.android.library.libxmtp.ConversationDebugInfo.CommitLogForkStatus
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DecodedMessage.MessageDeliveryStatus
import org.xmtp.android.library.libxmtp.DecodedMessage.SortDirection
import org.xmtp.android.library.libxmtp.DecodedMessageV2
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.GroupMembershipResult
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.android.library.libxmtp.PermissionOption
import org.xmtp.android.library.libxmtp.PermissionPolicySet
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.proto.keystore.api.v1.Keystore
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationMetadata
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiDirection
import uniffi.xmtpv3.FfiGroupPermissions
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMessageDisappearingSettings
import uniffi.xmtpv3.FfiMetadataField
import uniffi.xmtpv3.FfiPermissionUpdateType
import uniffi.xmtpv3.FfiSubscribeException
import java.util.Date

class Group(
    val client: Client,
    private val libXMTPGroup: FfiConversation,
    private val ffiLastMessage: FfiMessage? = null,
    private val ffiIsCommitLogForked: Boolean? = null,
) {
    val id: String
        get() = libXMTPGroup.id().toHex()

    val topic: String
        get() = Topic.groupMessage(id).description

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    val createdAtNs: Long
        get() = libXMTPGroup.createdAtNs()

    val lastActivityNs: Long
        get() = ffiLastMessage?.sentAtNs ?: createdAtNs

    private suspend fun metadata(): FfiConversationMetadata =
        withContext(Dispatchers.IO) { libXMTPGroup.groupMetadata() }

    suspend fun permissions(): FfiGroupPermissions = withContext(Dispatchers.IO) { libXMTPGroup.groupPermissions() }

    @Deprecated(
        message = "Use suspend name()",
        replaceWith = ReplaceWith("name()"),
    )
    val name: String
        get() = libXMTPGroup.groupName()

    suspend fun name(): String = withContext(Dispatchers.IO) { libXMTPGroup.groupName() }

    @Deprecated(
        message = "Use suspend imageUrl()",
        replaceWith = ReplaceWith("imageUrl()"),
    )
    val imageUrl: String
        get() = libXMTPGroup.groupImageUrlSquare()

    suspend fun imageUrl(): String = withContext(Dispatchers.IO) { libXMTPGroup.groupImageUrlSquare() }

    @Deprecated(
        message = "Use suspend description()",
        replaceWith = ReplaceWith("description()"),
    )
    val description: String
        get() = libXMTPGroup.groupDescription()

    suspend fun description(): String = withContext(Dispatchers.IO) { libXMTPGroup.groupDescription() }

    @Deprecated(
        message = "Use suspend disappearingMessageSettings()",
        replaceWith = ReplaceWith("disappearingMessageSettings()"),
    )
    val disappearingMessageSettings: DisappearingMessageSettings?
        get() =
            runCatching {
                libXMTPGroup.takeIf { isDisappearingMessagesEnabled }?.let { group ->
                    group.conversationMessageDisappearingSettings()?.let {
                        DisappearingMessageSettings.createFromFfi(it)
                    }
                }
            }.getOrNull()

    suspend fun disappearingMessageSettings(): DisappearingMessageSettings? =
        withContext(Dispatchers.IO) {
            runCatching {
                libXMTPGroup.takeIf { isDisappearingMessagesEnabled() }?.let { group ->
                    group.conversationMessageDisappearingSettings()?.let {
                        DisappearingMessageSettings.createFromFfi(it)
                    }
                }
            }.getOrNull()
        }

    @Deprecated(
        message = "Use suspend isDisappearingMessagesEnabled()",
    )
    val isDisappearingMessagesEnabled: Boolean
        get() = libXMTPGroup.isConversationMessageDisappearingEnabled()

    suspend fun isDisappearingMessagesEnabled(): Boolean =
        withContext(Dispatchers.IO) { libXMTPGroup.isConversationMessageDisappearingEnabled() }

    suspend fun send(text: String): String =
        withContext(Dispatchers.IO) {
            send(encodeContent(content = text, options = null))
        }

    suspend fun <T> send(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            val preparedMessage = encodeContent(content = content, options = options)
            send(preparedMessage)
        }

    suspend fun send(encodedContent: EncodedContent): String =
        withContext(Dispatchers.IO) {
            val messageId = libXMTPGroup.send(contentBytes = encodedContent.toByteArray())
            messageId.toHex()
        }

    fun <T> encodeContent(
        content: T,
        options: SendOptions?,
    ): EncodedContent {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(
            codec: Codec,
            content: T,
        ): EncodedContent = codec.encode(content)
        try {
            @Suppress("UNCHECKED_CAST")
            var encoded = encode(codec as ContentCodec<T>, content)
            val fallback = codec.fallback(content)
            if (!fallback.isNullOrBlank()) {
                encoded = encoded.toBuilder().also { it.fallback = fallback }.build()
            }
            val compression = options?.compression
            if (compression != null) {
                encoded = encoded.compress(compression)
            }
            return encoded
        } catch (e: Exception) {
            throw XMTPException("Codec type is not registered")
        }
    }

    suspend fun prepareMessage(encodedContent: EncodedContent): String =
        withContext(Dispatchers.IO) {
            libXMTPGroup.sendOptimistic(encodedContent.toByteArray()).toHex()
        }

    suspend fun <T> prepareMessage(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            val encodeContent = encodeContent(content = content, options = options)
            libXMTPGroup.sendOptimistic(encodeContent.toByteArray()).toHex()
        }

    suspend fun publishMessages() = withContext(Dispatchers.IO) { libXMTPGroup.publishMessages() }

    suspend fun sync() = withContext(Dispatchers.IO) { libXMTPGroup.sync() }

    suspend fun lastMessage(): DecodedMessage? =
        withContext(Dispatchers.IO) {
            if (ffiLastMessage != null) {
                DecodedMessage.create(ffiLastMessage)
            } else {
                messages(limit = 1).firstOrNull()
            }
        }

    fun commitLogForkStatus(): CommitLogForkStatus =
        when (ffiIsCommitLogForked) {
            true -> CommitLogForkStatus.FORKED
            false -> CommitLogForkStatus.NOT_FORKED
            null -> CommitLogForkStatus.UNKNOWN
        }

    suspend fun messages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> =
        withContext(Dispatchers.IO) {
            libXMTPGroup
                .findMessages(
                    opts =
                        FfiListMessagesOptions(
                            sentBeforeNs = beforeNs,
                            sentAfterNs = afterNs,
                            limit = limit?.toLong(),
                            deliveryStatus =
                                when (deliveryStatus) {
                                    MessageDeliveryStatus.PUBLISHED ->
                                        FfiDeliveryStatus.PUBLISHED

                                    MessageDeliveryStatus.UNPUBLISHED ->
                                        FfiDeliveryStatus.UNPUBLISHED

                                    MessageDeliveryStatus.FAILED ->
                                        FfiDeliveryStatus.FAILED

                                    else -> null
                                },
                            direction =
                                when (direction) {
                                    SortDirection.ASCENDING ->
                                        FfiDirection.ASCENDING

                                    else -> FfiDirection.DESCENDING
                                },
                            contentTypes = null,
                        ),
                ).mapNotNull { DecodedMessage.create(it) }
        }

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> =
        withContext(Dispatchers.IO) {
            val ffiMessageWithReactions =
                libXMTPGroup.findMessagesWithReactions(
                    opts =
                        FfiListMessagesOptions(
                            sentBeforeNs = beforeNs,
                            sentAfterNs = afterNs,
                            limit = limit?.toLong(),
                            deliveryStatus =
                                when (deliveryStatus) {
                                    MessageDeliveryStatus.PUBLISHED ->
                                        FfiDeliveryStatus.PUBLISHED

                                    MessageDeliveryStatus.UNPUBLISHED ->
                                        FfiDeliveryStatus.UNPUBLISHED

                                    MessageDeliveryStatus.FAILED ->
                                        FfiDeliveryStatus.FAILED

                                    else -> null
                                },
                            when (direction) {
                                SortDirection.ASCENDING ->
                                    FfiDirection.ASCENDING

                                else -> FfiDirection.DESCENDING
                            },
                            contentTypes = null,
                        ),
                )

            ffiMessageWithReactions.mapNotNull { ffiMessageWithReaction ->
                DecodedMessage.create(ffiMessageWithReaction)
            }
        }

    suspend fun enrichedMessages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessageV2> =
        withContext(Dispatchers.IO) {
            libXMTPGroup
                .findMessagesV2(
                    opts =
                        FfiListMessagesOptions(
                            sentBeforeNs = beforeNs,
                            sentAfterNs = afterNs,
                            limit = limit?.toLong(),
                            deliveryStatus =
                                when (deliveryStatus) {
                                    MessageDeliveryStatus.PUBLISHED ->
                                        FfiDeliveryStatus.PUBLISHED

                                    MessageDeliveryStatus.UNPUBLISHED ->
                                        FfiDeliveryStatus.UNPUBLISHED

                                    MessageDeliveryStatus.FAILED ->
                                        FfiDeliveryStatus.FAILED

                                    else -> null
                                },
                            direction =
                                when (direction) {
                                    SortDirection.ASCENDING ->
                                        FfiDirection.ASCENDING

                                    else -> FfiDirection.DESCENDING
                                },
                            contentTypes = null,
                        ),
                ).mapNotNull { DecodedMessageV2.create(it) }
        }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? =
        withContext(Dispatchers.IO) {
            val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
            DecodedMessage.create(message)
        }

    suspend fun updateConsentState(state: ConsentState) =
        withContext(Dispatchers.IO) {
            val consentState = ConsentState.toFfiConsentState(state)
            libXMTPGroup.updateConsentState(consentState)
        }

    suspend fun consentState(): ConsentState =
        withContext(Dispatchers.IO) {
            ConsentState.fromFfiConsentState(libXMTPGroup.consentState())
        }

    suspend fun isActive(): Boolean = withContext(Dispatchers.IO) { libXMTPGroup.isActive() }

    suspend fun addedByInboxId(): InboxId = withContext(Dispatchers.IO) { libXMTPGroup.addedByInboxId() }

    suspend fun permissionPolicySet(): PermissionPolicySet =
        withContext(Dispatchers.IO) {
            PermissionPolicySet.fromFfiPermissionPolicySet(permissions().policySet())
        }

    suspend fun creatorInboxId(): InboxId = withContext(Dispatchers.IO) { metadata().creatorInboxId() }

    suspend fun isCreator(): Boolean = withContext(Dispatchers.IO) { metadata().creatorInboxId() == client.inboxId }

    suspend fun addMembersByIdentity(identities: List<PublicIdentity>): GroupMembershipResult =
        withContext(Dispatchers.IO) {
            try {
                val result = libXMTPGroup.addMembers(identities.map { it.ffiPrivate })
                GroupMembershipResult(result)
            } catch (e: Exception) {
                throw XMTPException("Unable to add member", e)
            }
        }

    suspend fun removeMembersByIdentity(identities: List<PublicIdentity>) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.removeMembers(identities.map { it.ffiPrivate })
            } catch (e: Exception) {
                throw XMTPException("Unable to remove member", e)
            }
        }

    suspend fun addMembers(inboxIds: List<InboxId>): GroupMembershipResult =
        withContext(Dispatchers.IO) {
            validateInboxIds(inboxIds)
            try {
                val result = libXMTPGroup.addMembersByInboxId(inboxIds)
                GroupMembershipResult(result)
            } catch (e: Exception) {
                throw XMTPException("Unable to add member", e)
            }
        }

    suspend fun removeMembers(inboxIds: List<InboxId>) =
        withContext(Dispatchers.IO) {
            validateInboxIds(inboxIds)
            try {
                libXMTPGroup.removeMembersByInboxId(inboxIds)
            } catch (e: Exception) {
                throw XMTPException("Unable to remove member", e)
            }
        }

    suspend fun members(): List<Member> = withContext(Dispatchers.IO) { libXMTPGroup.listMembers().map { Member(it) } }

    suspend fun peerInboxIds(): List<InboxId> =
        withContext(Dispatchers.IO) {
            val ids = members().map { it.inboxId }.toMutableList()
            ids.remove(client.inboxId)
            ids
        }

    suspend fun updateName(name: String) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.updateGroupName(name)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to update group name", e)
            }
        }

    suspend fun updateImageUrl(imageUrl: String) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.updateGroupImageUrlSquare(imageUrl)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to update image url", e)
            }
        }

    suspend fun updateDescription(description: String) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.updateGroupDescription(description)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to update group description", e)
            }
        }

    suspend fun clearDisappearingMessageSettings() =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.removeConversationMessageDisappearingSettings()
            } catch (e: Exception) {
                throw XMTPException(
                    "Permission denied: Unable to clear group message expiration",
                    e,
                )
            }
        }

    suspend fun updateDisappearingMessageSettings(disappearingMessageSettings: DisappearingMessageSettings?) =
        withContext(Dispatchers.IO) {
            try {
                if (disappearingMessageSettings == null) {
                    clearDisappearingMessageSettings()
                } else {
                    libXMTPGroup.updateConversationMessageDisappearingSettings(
                        FfiMessageDisappearingSettings(
                            disappearingMessageSettings.disappearStartingAtNs,
                            disappearingMessageSettings.retentionDurationInNs,
                        ),
                    )
                }
            } catch (e: Exception) {
                throw XMTPException(
                    "Permission denied: Unable to update group message expiration",
                    e,
                )
            }
        }

    suspend fun updateAddMemberPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.ADD_MEMBER,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                null,
            )
        }

    suspend fun updateRemoveMemberPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.REMOVE_MEMBER,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                null,
            )
        }

    suspend fun updateAddAdminPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.ADD_ADMIN,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                null,
            )
        }

    suspend fun updateRemoveAdminPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.REMOVE_ADMIN,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                null,
            )
        }

    suspend fun updateNamePermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.UPDATE_METADATA,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                FfiMetadataField.GROUP_NAME,
            )
        }

    suspend fun updateDescriptionPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.UPDATE_METADATA,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                FfiMetadataField.DESCRIPTION,
            )
        }

    suspend fun updateImageUrlPermission(newPermissionOption: PermissionOption) =
        withContext(Dispatchers.IO) {
            libXMTPGroup.updatePermissionPolicy(
                FfiPermissionUpdateType.UPDATE_METADATA,
                PermissionOption.toFfiPermissionPolicy(newPermissionOption),
                FfiMetadataField.IMAGE_URL_SQUARE,
            )
        }

    suspend fun isAdmin(inboxId: InboxId): Boolean = withContext(Dispatchers.IO) { libXMTPGroup.isAdmin(inboxId) }

    suspend fun isSuperAdmin(inboxId: InboxId): Boolean =
        withContext(Dispatchers.IO) { libXMTPGroup.isSuperAdmin(inboxId) }

    suspend fun addAdmin(inboxId: InboxId) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.addAdmin(inboxId)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to add admin", e)
            }
        }

    suspend fun removeAdmin(inboxId: InboxId) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.removeAdmin(inboxId)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to remove admin", e)
            }
        }

    suspend fun addSuperAdmin(inboxId: InboxId) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.addSuperAdmin(inboxId)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to add super admin", e)
            }
        }

    suspend fun removeSuperAdmin(inboxId: InboxId) =
        withContext(Dispatchers.IO) {
            try {
                libXMTPGroup.removeSuperAdmin(inboxId)
            } catch (e: Exception) {
                throw XMTPException("Permission denied: Unable to remove super admin", e)
            }
        }

    suspend fun listAdmins(): List<InboxId> = withContext(Dispatchers.IO) { libXMTPGroup.adminList() }

    suspend fun listSuperAdmins(): List<InboxId> = withContext(Dispatchers.IO) { libXMTPGroup.superAdminList() }

    // Returns null if group is not paused, otherwise the min version required to unpause this group
    suspend fun pausedForVersion(): String? = withContext(Dispatchers.IO) { libXMTPGroup.pausedForVersion() }

    fun streamMessages(onClose: (() -> Unit)? = null): Flow<DecodedMessage> =
        callbackFlow {
            val messageCallback =
                object : FfiMessageCallback {
                    override fun onMessage(message: FfiMessage) {
                        try {
                            val decodedMessage = DecodedMessage.create(message)
                            if (decodedMessage != null) {
                                trySend(decodedMessage)
                            } else {
                                Log.w(
                                    "XMTP Group stream",
                                    "Failed to decode message: id=${message.id.toHex()}, " +
                                        "conversationId=${message.conversationId.toHex()}, " +
                                        "senderInboxId=${message.senderInboxId}",
                                )
                            }
                        } catch (e: Exception) {
                            Log.e(
                                "XMTP Group stream",
                                "Error decoding message: id=${message.id.toHex()}, " +
                                    "conversationId=${message.conversationId.toHex()}, " +
                                    "senderInboxId=${message.senderInboxId}",
                                e,
                            )
                        }
                    }

                    override fun onError(error: FfiSubscribeException) {
                        Log.e("XMTP Group stream", "Stream error: ${error.message}", error)
                    }

                    override fun onClose() {
                        onClose?.invoke()
                        close()
                    }
                }

            val stream = libXMTPGroup.stream(messageCallback)
            awaitClose { stream.end() }
        }

    suspend fun getHmacKeys(): Keystore.GetConversationHmacKeysResponse =
        withContext(Dispatchers.IO) {
            val hmacKeysResponse = Keystore.GetConversationHmacKeysResponse.newBuilder()
            val conversations = libXMTPGroup.getHmacKeys()
            conversations.iterator().forEach {
                val hmacKeys = Keystore.GetConversationHmacKeysResponse.HmacKeys.newBuilder()
                it.value.forEach { key ->
                    val hmacKeyData =
                        Keystore.GetConversationHmacKeysResponse.HmacKeyData.newBuilder()
                    hmacKeyData.hmacKey = key.key.toByteString()
                    hmacKeyData.thirtyDayPeriodsSinceEpoch = key.epoch.toInt()
                    hmacKeys.addValues(hmacKeyData)
                }
                hmacKeysResponse.putHmacKeys(
                    Topic.groupMessage(it.key.toHex()).description,
                    hmacKeys.build(),
                )
            }
            hmacKeysResponse.build()
        }

    suspend fun countMessages(
        beforeNs: Long? = null,
        afterNs: Long? = null,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): Long =
        withContext(Dispatchers.IO) {
            libXMTPGroup.countMessages(
                opts =
                    FfiListMessagesOptions(
                        sentBeforeNs = beforeNs,
                        sentAfterNs = afterNs,
                        limit = null,
                        deliveryStatus =
                            when (deliveryStatus) {
                                MessageDeliveryStatus.PUBLISHED ->
                                    FfiDeliveryStatus.PUBLISHED

                                MessageDeliveryStatus.UNPUBLISHED ->
                                    FfiDeliveryStatus.UNPUBLISHED

                                MessageDeliveryStatus.FAILED ->
                                    FfiDeliveryStatus.FAILED

                                else -> null
                            },
                        direction = null,
                        contentTypes = null,
                    ),
            )
        }

    fun getPushTopics(): List<String> = listOf(topic)

    suspend fun getDebugInformation(): ConversationDebugInfo =
        withContext(Dispatchers.IO) {
            ConversationDebugInfo(libXMTPGroup.conversationDebugInfo())
        }

    suspend fun getLastReadTimes(): Map<InboxId, Long> = withContext(Dispatchers.IO) { libXMTPGroup.getLastReadTimes() }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (javaClass != other?.javaClass) return false

        other as Group

        return id == other.id
    }

    override fun hashCode(): Int = id.hashCode()
}
