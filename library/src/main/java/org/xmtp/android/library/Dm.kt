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
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.SortDirection
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationMetadata
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiDirection
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import java.util.Date
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

class Dm(val client: Client, private val libXMTPGroup: FfiConversation) {
    val id: String
        get() = libXMTPGroup.id().toHex()

    val topic: String
        get() = Topic.groupMessage(id).description

    val createdAt: Date
        get() = Date(libXMTPGroup.createdAtNs() / 1_000_000)

    private val metadata: FfiConversationMetadata
        get() = libXMTPGroup.groupMetadata()

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

    fun messages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = SortDirection.SORT_DIRECTION_DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        return libXMTPGroup.findMessages(
            opts = FfiListMessagesOptions(
                sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                },
                direction = when (direction) {
                    SortDirection.SORT_DIRECTION_ASCENDING -> FfiDirection.ASCENDING
                    else -> FfiDirection.DESCENDING
                }
            )
        ).mapNotNull {
            MessageV3(client, it).decodeOrNull()
        }
    }

    fun decryptedMessages(
        limit: Int? = null,
        before: Date? = null,
        after: Date? = null,
        direction: PagingInfoSortDirection = SortDirection.SORT_DIRECTION_DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecryptedMessage> {
        return libXMTPGroup.findMessages(
            opts = FfiListMessagesOptions(
                sentBeforeNs = before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                sentAfterNs = after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit = limit?.toLong(),
                deliveryStatus = when (deliveryStatus) {
                    MessageDeliveryStatus.PUBLISHED -> FfiDeliveryStatus.PUBLISHED
                    MessageDeliveryStatus.UNPUBLISHED -> FfiDeliveryStatus.UNPUBLISHED
                    MessageDeliveryStatus.FAILED -> FfiDeliveryStatus.FAILED
                    else -> null
                },
                direction = when (direction) {
                    SortDirection.SORT_DIRECTION_ASCENDING -> FfiDirection.ASCENDING
                    else -> FfiDirection.DESCENDING
                }
            )
        ).mapNotNull {
            MessageV3(client, it).decryptOrNull()
        }
    }

    suspend fun processMessage(envelopeBytes: ByteArray): MessageV3 {
        val message = libXMTPGroup.processStreamedConversationMessage(envelopeBytes)
        return MessageV3(client, message)
    }

    fun creatorInboxId(): String {
        return metadata.creatorInboxId()
    }

    fun isCreator(): Boolean {
        return metadata.creatorInboxId() == client.inboxId
    }

    suspend fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    suspend fun peerInboxId(): String {
        val ids = members().map { it.inboxId }.toMutableList()
        ids.remove(client.inboxId)
        return ids.first()
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

    suspend fun updateConsentState(state: ConsentState) {
        if (client.hasV2Client) {
            when (state) {
                ConsentState.ALLOWED -> client.contacts.allowGroups(groupIds = listOf(id))
                ConsentState.DENIED -> client.contacts.denyGroups(groupIds = listOf(id))
                ConsentState.UNKNOWN -> Unit
            }
        }

        val consentState = ConsentState.toFfiConsentState(state)
        libXMTPGroup.updateConsentState(consentState)
    }

    fun consentState(): ConsentState {
        return ConsentState.fromFfiConsentState(libXMTPGroup.consentState())
    }
}
