package org.xmtp.android.library

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
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
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.proto.keystore.api.v1.Keystore
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
    private val ffiIsCommitLogForked: Boolean? = null
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

    val peerInboxId: InboxId
        get() = libXMTPGroup.dmPeerInboxId() ?: throw XMTPException("peerInboxId not found")

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

    suspend fun lastMessage(): DecodedMessage? {
        return if (ffiLastMessage != null) {
            DecodedMessage.create(ffiLastMessage)
        } else {
            messages(limit = 1).firstOrNull()
        }
    }

    fun commitLogForkStatus(): CommitLogForkStatus {
        return when (ffiIsCommitLogForked) {
            true -> CommitLogForkStatus.FORKED
            false -> CommitLogForkStatus.NOT_FORKED
            null -> CommitLogForkStatus.UNKNOWN
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

    suspend fun messagesV2(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
    ): List<DecodedMessageV2> {
        return libXMTPGroup.findMessagesV2(
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
            DecodedMessageV2.create(it)
        }
    }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? {
        val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
        return DecodedMessage.create(message)
    }

    suspend fun creatorInboxId(): InboxId {
        return metadata().creatorInboxId()
    }

    suspend fun isCreator(): Boolean {
        return metadata().creatorInboxId() == client.inboxId
    }

    fun isActive(): Boolean {
        return libXMTPGroup.isActive()
    }

    suspend fun members(): List<Member> {
        return libXMTPGroup.listMembers().map { Member(it) }
    }

    fun streamMessages(onClose: (() -> Unit)? = null): Flow<DecodedMessage> = callbackFlow {
        val messageCallback = object : FfiMessageCallback {
            override fun onMessage(message: FfiMessage) {
                try {
                    val decodedMessage = DecodedMessage.create(message)
                    if (decodedMessage != null) {
                        trySend(decodedMessage)
                    } else {
                        Log.w(
                            "XMTP Dm stream",
                            "Failed to decode message: id=${message.id.toHex()}, " + "conversationId=${message.conversationId.toHex()}, " + "senderInboxId=${message.senderInboxId}"
                        )
                    }
                } catch (e: Exception) {
                    Log.e(
                        "XMTP Dm stream",
                        "Error decoding message: id=${message.id.toHex()}, " + "conversationId=${message.conversationId.toHex()}, " + "senderInboxId=${message.senderInboxId}",
                        e
                    )
                }
            }

            override fun onError(error: FfiSubscribeException) {
                Log.e("XMTP Dm stream", "Stream error: ${error.message}", error)
            }

            override fun onClose() {
                onClose?.invoke()
                close()
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

    // Returns null if dm is not paused, otherwise the min version required to unpause this dm
    fun pausedForVersion(): String? {
        return libXMTPGroup.pausedForVersion()
    }

    fun getHmacKeys(): Keystore.GetConversationHmacKeysResponse {
        val hmacKeysResponse = Keystore.GetConversationHmacKeysResponse.newBuilder()
        val conversations = libXMTPGroup.getHmacKeys()
        conversations.iterator().forEach {
            val hmacKeys = Keystore.GetConversationHmacKeysResponse.HmacKeys.newBuilder()
            it.value.forEach { key ->
                val hmacKeyData = Keystore.GetConversationHmacKeysResponse.HmacKeyData.newBuilder()
                hmacKeyData.hmacKey = key.key.toByteString()
                hmacKeyData.thirtyDayPeriodsSinceEpoch = key.epoch.toInt()
                hmacKeys.addValues(hmacKeyData)
            }
            hmacKeysResponse.putHmacKeys(
                Topic.groupMessage(it.key.toHex()).description,
                hmacKeys.build()
            )
        }
        return hmacKeysResponse.build()
    }

    suspend fun getPushTopics(): List<String> {
        val duplicates = libXMTPGroup.findDuplicateDms()
        val topicIds = duplicates.map { it.id().toHex() }.toMutableList()
        topicIds.add(id)
        return topicIds.map { Topic.groupMessage(it).description }
    }

    suspend fun getDebugInformation(): ConversationDebugInfo {
        return ConversationDebugInfo(libXMTPGroup.conversationDebugInfo())
    }

    fun getLastReadTimes(): Map<InboxId, Long> {
        return libXMTPGroup.getLastReadTimes()
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (javaClass != other?.javaClass) return false

        other as Dm

        return id == other.id
    }

    override fun hashCode(): Int {
        return id.hashCode()
    }
}
