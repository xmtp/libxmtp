package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DecodedMessage.MessageDeliveryStatus
import org.xmtp.android.library.libxmtp.DecodedMessage.SortDirection
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.android.library.libxmtp.PermissionOption
import org.xmtp.android.library.libxmtp.PermissionPolicySet
import org.xmtp.android.library.messages.Topic
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
) {
    val id: String
        get() = libXMTPGroup.id().toHex()

    val topic: String
        get() = Topic.groupMessage(id).description

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    private suspend fun metadata(): FfiConversationMetadata {
        return libXMTPGroup.groupMetadata()
    }

    private val permissions: FfiGroupPermissions
        get() = libXMTPGroup.groupPermissions()

    val name: String
        get() = libXMTPGroup.groupName()

    val imageUrl: String
        get() = libXMTPGroup.groupImageUrlSquare()

    val description: String
        get() = libXMTPGroup.groupDescription()

    val disappearingMessageSettings: DisappearingMessageSettings?
        get() = runCatching {
            libXMTPGroup.takeIf { isDisappearingMessagesEnabled }
                ?.let { group ->
                    group.conversationMessageDisappearingSettings()
                        ?.let { DisappearingMessageSettings.createFromFfi(it) }
                }
        }.getOrNull()

    val isDisappearingMessagesEnabled: Boolean
        get() = libXMTPGroup.isConversationMessageDisappearingEnabled()

    suspend fun send(text: String): String {
        return send(encodeContent(content = text, options = null))
    }

    suspend fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = encodeContent(content = content, options = options)
        return send(preparedMessage)
    }

    suspend fun send(encodedContent: EncodedContent): String {
        val messageId = libXMTPGroup.send(contentBytes = encodedContent.toByteArray())
        return messageId.toHex()
    }

    fun <T> encodeContent(content: T, options: SendOptions?): EncodedContent {
        val codec = Client.codecRegistry.find(options?.contentType)
        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: T): EncodedContent {
            return codec.encode(content)
        }
        try {
            @Suppress("UNCHECKED_CAST")
            var encoded = encode(codec as ContentCodec<T>, content)
            val fallback = codec.fallback(content)
            if (!fallback.isNullOrBlank()) {
                encoded = encoded.toBuilder().also {
                    it.fallback = fallback
                }.build()
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

    fun prepareMessage(encodedContent: EncodedContent): String {
        return libXMTPGroup.sendOptimistic(encodedContent.toByteArray()).toHex()
    }

    fun <T> prepareMessage(content: T, options: SendOptions? = null): String {
        val encodeContent = encodeContent(content = content, options = options)
        return libXMTPGroup.sendOptimistic(encodeContent.toByteArray()).toHex()
    }

    suspend fun publishMessages() {
        libXMTPGroup.publishMessages()
    }

    suspend fun sync() {
        libXMTPGroup.sync()
    }

    suspend fun lastMessage(): DecodedMessage? {
        return if (ffiLastMessage != null) {
            DecodedMessage.create(ffiLastMessage)
        } else {
            messages(limit = 1).firstOrNull()
        }
    }

    suspend fun messages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        return libXMTPGroup.findMessages(
            opts = FfiListMessagesOptions(
                sentBeforeNs = beforeNs,
                sentAfterNs = afterNs,
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                },
                direction = when (direction) {
                    SortDirection.ASCENDING -> FfiDirection.ASCENDING
                    else -> FfiDirection.DESCENDING
                },
                contentTypes = null
            )
        ).mapNotNull {
            DecodedMessage.create(it)
        }
    }

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        val ffiMessageWithReactions = libXMTPGroup.findMessagesWithReactions(
            opts = FfiListMessagesOptions(
                sentBeforeNs = beforeNs,
                sentAfterNs = afterNs,
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                },
                when (direction) {
                    SortDirection.ASCENDING -> FfiDirection.ASCENDING
                    else -> FfiDirection.DESCENDING
                },
                contentTypes = null
            )
        )

        return ffiMessageWithReactions.mapNotNull { ffiMessageWithReaction ->
            DecodedMessage.create(ffiMessageWithReaction)
        }
    }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? {
        val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
        return DecodedMessage.create(message)
    }

    fun updateConsentState(state: ConsentState) {
        val consentState = ConsentState.toFfiConsentState(state)
        libXMTPGroup.updateConsentState(consentState)
    }

    fun consentState(): ConsentState {
        return ConsentState.fromFfiConsentState(libXMTPGroup.consentState())
    }

    fun isActive(): Boolean {
        return libXMTPGroup.isActive()
    }

    fun addedByInboxId(): InboxId {
        return libXMTPGroup.addedByInboxId()
    }

    fun permissionPolicySet(): PermissionPolicySet {
        return PermissionPolicySet.fromFfiPermissionPolicySet(permissions.policySet())
    }

    suspend fun creatorInboxId(): InboxId {
        return metadata().creatorInboxId()
    }

    suspend fun isCreator(): Boolean {
        return metadata().creatorInboxId() == client.inboxId
    }

    suspend fun addMembersByIdentity(identities: List<PublicIdentity>) {
        try {
            libXMTPGroup.addMembers(identities.map { it.ffiPrivate })
        } catch (e: Exception) {
            throw XMTPException("Unable to add member", e)
        }
    }

    suspend fun removeMembersByIdentity(identities: List<PublicIdentity>) {
        try {
            libXMTPGroup.removeMembers(identities.map { it.ffiPrivate })
        } catch (e: Exception) {
            throw XMTPException("Unable to remove member", e)
        }
    }

    suspend fun addMembers(inboxIds: List<InboxId>) {
        validateInboxIds(inboxIds)
        try {
            libXMTPGroup.addMembersByInboxId(inboxIds)
        } catch (e: Exception) {
            throw XMTPException("Unable to add member", e)
        }
    }

    suspend fun removeMembers(inboxIds: List<InboxId>) {
        validateInboxIds(inboxIds)
        try {
            libXMTPGroup.removeMembersByInboxId(inboxIds)
        } catch (e: Exception) {
            throw XMTPException("Unable to remove member", e)
        }
    }

    suspend fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    suspend fun peerInboxIds(): List<InboxId> {
        val ids = members().map { it.inboxId }.toMutableList()
        ids.remove(client.inboxId)
        return ids
    }

    suspend fun updateName(name: String) {
        try {
            return libXMTPGroup.updateGroupName(name)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update group name", e)
        }
    }

    suspend fun updateImageUrl(imageUrl: String) {
        try {
            return libXMTPGroup.updateGroupImageUrlSquare(imageUrl)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update image url", e)
        }
    }

    suspend fun updateDescription(description: String) {
        try {
            return libXMTPGroup.updateGroupDescription(description)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update group description", e)
        }
    }

    suspend fun clearDisappearingMessageSettings() {
        try {
            libXMTPGroup.removeConversationMessageDisappearingSettings()
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to clear group message expiration", e)
        }
    }

    suspend fun updateDisappearingMessageSettings(disappearingMessageSettings: DisappearingMessageSettings?) {
        try {
            if (disappearingMessageSettings == null) {
                clearDisappearingMessageSettings()
            } else {
                libXMTPGroup.updateConversationMessageDisappearingSettings(
                    FfiMessageDisappearingSettings(
                        disappearingMessageSettings.disappearStartingAtNs,
                        disappearingMessageSettings.retentionDurationInNs
                    )
                )
            }
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update group message expiration", e)
        }
    }

    suspend fun updateAddMemberPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.ADD_MEMBER,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            null
        )
    }

    suspend fun updateRemoveMemberPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.REMOVE_MEMBER,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            null
        )
    }

    suspend fun updateAddAdminPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.ADD_ADMIN,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            null
        )
    }

    suspend fun updateRemoveAdminPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.REMOVE_ADMIN,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            null
        )
    }

    suspend fun updateNamePermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.GROUP_NAME
        )
    }

    suspend fun updateDescriptionPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.DESCRIPTION
        )
    }

    suspend fun updateImageUrlPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.IMAGE_URL_SQUARE
        )
    }

    fun isAdmin(inboxId: InboxId): Boolean {
        return libXMTPGroup.isAdmin(inboxId)
    }

    fun isSuperAdmin(inboxId: InboxId): Boolean {
        return libXMTPGroup.isSuperAdmin(inboxId)
    }

    suspend fun addAdmin(inboxId: InboxId) {
        try {
            libXMTPGroup.addAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to add admin", e)
        }
    }

    suspend fun removeAdmin(inboxId: InboxId) {
        try {
            libXMTPGroup.removeAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to remove admin", e)
        }
    }

    suspend fun addSuperAdmin(inboxId: InboxId) {
        try {
            libXMTPGroup.addSuperAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to add super admin", e)
        }
    }

    suspend fun removeSuperAdmin(inboxId: InboxId) {
        try {
            libXMTPGroup.removeSuperAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to remove super admin", e)
        }
    }

    fun listAdmins(): List<InboxId> {
        return libXMTPGroup.adminList()
    }

    fun listSuperAdmins(): List<InboxId> {
        return libXMTPGroup.superAdminList()
    }

    fun streamMessages(): Flow<DecodedMessage> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
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
                                "senderInboxId=${message.senderInboxId}"
                        )
                    }
                } catch (e: Exception) {
                    Log.e(
                        "XMTP Group stream",
                        "Error decoding message: id=${message.id.toHex()}, " +
                            "conversationId=${message.conversationId.toHex()}, " +
                            "senderInboxId=${message.senderInboxId}",
                        e
                    )
                }
            }

            override fun onError(error: FfiSubscribeException) {
                Log.e("XMTP Group stream", "Stream error: ${error.message}", error)
            }
        }

        val stream = libXMTPGroup.stream(messageCallback)
        awaitClose { stream.end() }
    }
}
