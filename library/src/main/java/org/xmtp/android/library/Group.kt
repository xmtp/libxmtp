package org.xmtp.android.library

import android.util.Log
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.libxmtp.Message.MessageDeliveryStatus
import org.xmtp.android.library.libxmtp.Message.SortDirection
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
import uniffi.xmtpv3.FfiMetadataField
import uniffi.xmtpv3.FfiPermissionUpdateType
import uniffi.xmtpv3.FfiSubscribeException

import java.util.Date

class Group(
    private val clientInboxId: String,
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

    val imageUrlSquare: String
        get() = libXMTPGroup.groupImageUrlSquare()

    val description: String
        get() = libXMTPGroup.groupDescription()

    val pinnedFrameUrl: String
        get() = libXMTPGroup.groupPinnedFrameUrl()

    suspend fun send(text: String): String {
        return send(encodeContent(content = text, options = null))
    }

    suspend fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = encodeContent(content = content, options = options)
        return send(preparedMessage)
    }

    suspend fun send(encodedContent: EncodedContent): String {
        if (consentState() == ConsentState.UNKNOWN) {
            updateConsentState(ConsentState.ALLOWED)
        }
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
        if (consentState() == ConsentState.UNKNOWN) {
            updateConsentState(ConsentState.ALLOWED)
        }
        return libXMTPGroup.sendOptimistic(encodedContent.toByteArray()).toHex()
    }

    fun <T> prepareMessage(content: T, options: SendOptions? = null): String {
        if (consentState() == ConsentState.UNKNOWN) {
            updateConsentState(ConsentState.ALLOWED)
        }
        val encodeContent = encodeContent(content = content, options = options)
        return libXMTPGroup.sendOptimistic(encodeContent.toByteArray()).toHex()
    }

    suspend fun publishMessages() {
        libXMTPGroup.publishMessages()
    }

    suspend fun sync() {
        libXMTPGroup.sync()
    }

    suspend fun lastMessage(): Message? {
        return if (ffiLastMessage != null) {
            Message.create(ffiLastMessage)
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
    ): List<Message> {
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
            Message.create(it)
        }
    }

    suspend fun processMessage(messageBytes: ByteArray): Message? {
        val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
        return Message.create(message)
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

    fun addedByInboxId(): String {
        return libXMTPGroup.addedByInboxId()
    }

    fun permissionPolicySet(): PermissionPolicySet {
        return PermissionPolicySet.fromFfiPermissionPolicySet(permissions.policySet())
    }

    suspend fun creatorInboxId(): String {
        return metadata().creatorInboxId()
    }

    suspend fun isCreator(): Boolean {
        return metadata().creatorInboxId() == clientInboxId
    }

    suspend fun addMembers(addresses: List<String>) {
        try {
            libXMTPGroup.addMembers(addresses)
        } catch (e: Exception) {
            throw XMTPException("Unable to add member", e)
        }
    }

    suspend fun removeMembers(addresses: List<String>) {
        try {
            libXMTPGroup.removeMembers(addresses)
        } catch (e: Exception) {
            throw XMTPException("Unable to remove member", e)
        }
    }

    suspend fun addMembersByInboxId(inboxIds: List<String>) {
        try {
            libXMTPGroup.addMembersByInboxId(inboxIds)
        } catch (e: Exception) {
            throw XMTPException("Unable to add member", e)
        }
    }

    suspend fun removeMembersByInboxId(inboxIds: List<String>) {
        try {
            libXMTPGroup.removeMembersByInboxId(inboxIds)
        } catch (e: Exception) {
            throw XMTPException("Unable to remove member", e)
        }
    }

    suspend fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    suspend fun peerInboxIds(): List<String> {
        val ids = members().map { it.inboxId }.toMutableList()
        ids.remove(clientInboxId)
        return ids
    }

    suspend fun updateGroupName(name: String) {
        try {
            return libXMTPGroup.updateGroupName(name)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update group name", e)
        }
    }

    suspend fun updateGroupImageUrlSquare(imageUrl: String) {
        try {
            return libXMTPGroup.updateGroupImageUrlSquare(imageUrl)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update image url", e)
        }
    }

    suspend fun updateGroupDescription(description: String) {
        try {
            return libXMTPGroup.updateGroupDescription(description)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update group description", e)
        }
    }

    suspend fun updateGroupPinnedFrameUrl(pinnedFrameUrl: String) {
        try {
            return libXMTPGroup.updateGroupPinnedFrameUrl(pinnedFrameUrl)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to update pinned frame", e)
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

    suspend fun updateGroupNamePermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.GROUP_NAME
        )
    }

    suspend fun updateGroupDescriptionPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.DESCRIPTION
        )
    }

    suspend fun updateGroupImageUrlSquarePermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.IMAGE_URL_SQUARE
        )
    }

    suspend fun updateGroupPinnedFrameUrlPermission(newPermissionOption: PermissionOption) {
        return libXMTPGroup.updatePermissionPolicy(
            FfiPermissionUpdateType.UPDATE_METADATA,
            PermissionOption.toFfiPermissionPolicy(newPermissionOption),
            FfiMetadataField.PINNED_FRAME_URL
        )
    }

    fun isAdmin(inboxId: String): Boolean {
        return libXMTPGroup.isAdmin(inboxId)
    }

    fun isSuperAdmin(inboxId: String): Boolean {
        return libXMTPGroup.isSuperAdmin(inboxId)
    }

    suspend fun addAdmin(inboxId: String) {
        try {
            libXMTPGroup.addAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to add admin", e)
        }
    }

    suspend fun removeAdmin(inboxId: String) {
        try {
            libXMTPGroup.removeAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to remove admin", e)
        }
    }

    suspend fun addSuperAdmin(inboxId: String) {
        try {
            libXMTPGroup.addSuperAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to add super admin", e)
        }
    }

    suspend fun removeSuperAdmin(inboxId: String) {
        try {
            libXMTPGroup.removeSuperAdmin(inboxId)
        } catch (e: Exception) {
            throw XMTPException("Permission denied: Unable to remove super admin", e)
        }
    }

    suspend fun listAdmins(): List<String> {
        return libXMTPGroup.adminList()
    }

    suspend fun listSuperAdmins(): List<String> {
        return libXMTPGroup.superAdminList()
    }

    fun streamMessages(): Flow<Message> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                val decodedMessage = Message.create(message)
                decodedMessage?.let {
                    trySend(it)
                }
            }

            override fun onError(error: FfiSubscribeException) {
                Log.e("XMTP Group stream", error.message.toString())
            }
        }

        val stream = libXMTPGroup.stream(messageCallback)
        awaitClose { stream.end() }
    }
}
