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
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.messages.Topic
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationMetadata
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiDirection
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMessageDisappearingSettings
import uniffi.xmtpv3.FfiSubscribeException
import java.util.Date

class Dm(
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

    val peerInboxId: String
        get() = libXMTPGroup.dmPeerInboxId()

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

    private suspend fun metadata(): FfiConversationMetadata {
        return libXMTPGroup.groupMetadata()
    }

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

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<Message> {
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
            Message.create(ffiMessageWithReaction)
        }
    }

    suspend fun processMessage(messageBytes: ByteArray): Message? {
        val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
        return Message.create(message)
    }

    suspend fun creatorInboxId(): String {
        return metadata().creatorInboxId()
    }

    suspend fun isCreator(): Boolean {
        return metadata().creatorInboxId() == client.inboxId
    }

    suspend fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    fun streamMessages(): Flow<Message> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                try {
                    val decodedMessage = Message.create(message)
                    if (decodedMessage != null) {
                        trySend(decodedMessage)
                    } else {
                        Log.w(
                            "XMTP Dm stream",
                            "Failed to decode message: id=${message.id.toHex()}, " +
                                "convoId=${message.convoId.toHex()}, " +
                                "senderInboxId=${message.senderInboxId}"
                        )
                    }
                } catch (e: Exception) {
                    Log.e(
                        "XMTP Dm stream",
                        "Error decoding message: id=${message.id.toHex()}, " +
                            "convoId=${message.convoId.toHex()}, " +
                            "senderInboxId=${message.senderInboxId}",
                        e
                    )
                }
            }

            override fun onError(error: FfiSubscribeException) {
                Log.e("XMTP Dm stream", "Stream error: ${error.message}", error)
            }
        }

        val stream = libXMTPGroup.stream(messageCallback)
        awaitClose { stream.end() }
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

    fun updateConsentState(state: ConsentState) {
        val consentState = ConsentState.toFfiConsentState(state)
        libXMTPGroup.updateConsentState(consentState)
    }

    fun consentState(): ConsentState {
        return ConsentState.fromFfiConsentState(libXMTPGroup.consentState())
    }
}
