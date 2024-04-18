package org.xmtp.android.library

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.codecs.compress
import org.xmtp.android.library.libxmtp.MessageV3
import org.xmtp.android.library.messages.DecryptedMessage
import org.xmtp.android.library.messages.MessageDeliveryStatus
import org.xmtp.android.library.messages.PagingInfoSortDirection
import org.xmtp.android.library.messages.Topic
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiGroup
import uniffi.xmtpv3.FfiGroupMetadata
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.GroupPermissions
import java.util.Date
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

class Group(val client: Client, private val libXMTPGroup: FfiGroup) {
    val id: ByteArray
        get() = libXMTPGroup.id()

    val topic: String
        get() = Topic.groupMessage(id.toHex()).description

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    private val metadata: FfiGroupMetadata
        get() = libXMTPGroup.groupMetadata()

    suspend fun send(text: String): String {
        return send(prepareMessage(content = text, options = null))
    }

    suspend fun <T> send(content: T, options: SendOptions? = null): String {
        val preparedMessage = prepareMessage(content = content, options = options)
        return send(preparedMessage)
    }

    suspend fun send(encodedContent: EncodedContent): String {
        if (client.contacts.consentList.groupState(groupId = id) == ConsentState.UNKNOWN) {
            client.contacts.allowGroup(groupIds = listOf(id))
        }
        val messageId = libXMTPGroup.send(contentBytes = encodedContent.toByteArray())
        return messageId.toHex()
    }

    fun <T> prepareMessage(content: T, options: SendOptions?): EncodedContent {
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

    fun addedByAddress(): String {
        return libXMTPGroup.addedByAddress()
    }

    fun permissionLevel(): GroupPermissions {
        return metadata.policyType()
    }

    fun isAdmin(): Boolean {
        return metadata.creatorAccountAddress().lowercase() == client.address.lowercase()
    }

    fun adminAddress(): String {
        return metadata.creatorAccountAddress()
    }

    suspend fun addMembers(addresses: List<String>) {
        try {
            libXMTPGroup.addMembers(addresses)
        } catch (e: Exception) {
            throw XMTPException("User does not have permissions", e)
        }
    }

    suspend fun removeMembers(addresses: List<String>) {
        try {
            libXMTPGroup.removeMembers(addresses)
        } catch (e: Exception) {
            throw XMTPException("User does not have permissions", e)
        }
    }

    fun memberAddresses(): List<String> {
        return libXMTPGroup.listMembers().map { it.accountAddress }
    }

    fun peerAddresses(): List<String> {
        val addresses = memberAddresses().map { it.lowercase() }.toMutableList()
        addresses.remove(client.address.lowercase())
        return addresses
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
