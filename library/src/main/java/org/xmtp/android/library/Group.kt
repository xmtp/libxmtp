package org.xmtp.android.library

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.android.library.libxmtp.MessageV3
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.android.library.messages.Topic
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiGroupMetadata
import uniffi.xmtpv3.FfiGroupPermissions
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMetadataField
import uniffi.xmtpv3.FfiPermissionUpdateType
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.PermissionOption
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.PermissionPolicySet
import java.util.Date
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

class Group(val client: Client, private val libXMTPGroup: FfiGroup) {
    val id: String
        get() = libXMTPGroup.id().toHex()

    val topic: String
        get() = Topic.groupMessage(id).description

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    private val metadata: FfiGroupMetadata
        get() = libXMTPGroup.groupMetadata()

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
        if (client.contacts.consentList.groupState(groupId = id) == ConsentState.UNKNOWN) {
            client.contacts.allowGroups(groupIds = listOf(id))
        }
        val messageId = libXMTPGroup.send(contentBytes = encodedContent.toByteArray())
        return messageId.toHex()
    }

    fun <T> encodeContent(content: T, options: SendOptions?): EncodedContent {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(codec: Codec, content: Any?): EncodedContent {
            val contentType = content as? T
            if (contentType != null) {
                return codec.encode(contentType)
            } else {
                throw XMTPException("Codec type is not registered")
            }
        }

        var encoded = encode(codec = codec as ContentCodec<T>, content = content)
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
    }

    suspend fun <T> prepareMessage(content: T, options: SendOptions? = null): String {
        if (client.contacts.consentList.groupState(groupId = id) == ConsentState.UNKNOWN) {
            client.contacts.allowGroups(groupIds = listOf(id))
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

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        val messages = libXMTPGroup.findMessages(
            opts = FfiListMessagesOptions(
                sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                }
            )
        ).mapNotNull {
            MessageV3(client, it).decodeOrNull()
        }

        return when (direction) {
            MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING -> messages
            else -> messages.reversed()
        }
    }

    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecryptedMessage> {
        val messages = libXMTPGroup.findMessages(
            opts = FfiListMessagesOptions(
                sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                }
            )
        ).mapNotNull {
            MessageV3(client, it).decryptOrNull()
        }

        return when (direction) {
            MessageApiOuterClass.SortDirection.SORT_DIRECTION_ASCENDING -> messages
            else -> messages.reversed()
        }
    }

    suspend fun processMessage(envelopeBytes: ByteArray): MessageV3 {
        val message = libXMTPGroup.processStreamedGroupMessage(envelopeBytes)
        return MessageV3(client, message)
    }

    fun isActive(): Boolean {
        return libXMTPGroup.isActive()
    }

    fun addedByInboxId(): String {
        return libXMTPGroup.addedByInboxId()
    }

    fun permissionPolicySet(): PermissionPolicySet {
        return PermissionPolicySet(permissions.policySet())
    }

    fun creatorInboxId(): String {
        return metadata.creatorInboxId()
    }

    fun isCreator(): Boolean {
        return metadata.creatorInboxId() == client.inboxId
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

    fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    fun peerInboxIds(): List<String> {
        val ids = members().map { it.inboxId }.toMutableList()
        ids.remove(client.inboxId)
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

    fun streamMessages(): Flow<DecodedMessage> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                val decodedMessage = MessageV3(client, message).decodeOrNull()
                decodedMessage?.let {
                    trySend(it)
                }
            }
        }

        val stream = libXMTPGroup.stream(messageCallback)
        awaitClose { stream.end() }
    }

    fun streamDecryptedMessages(): Flow<DecryptedMessage> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                val decryptedMessage = MessageV3(client, message).decryptOrNull()
                decryptedMessage?.let {
                    trySend(it)
                }
            }
        }

        val stream = libXMTPGroup.stream(messageCallback)
        awaitClose { stream.end() }
    }
}
