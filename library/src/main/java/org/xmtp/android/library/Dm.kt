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
import org.xmtp.android.library.libxmtp.DecodedMessage.SortBy
import org.xmtp.android.library.libxmtp.DecodedMessage.SortDirection
import org.xmtp.android.library.libxmtp.DecodedMessageV2
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.proto.keystore.api.v1.Keystore
import uniffi.xmtpv3.FfiContentType
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationMetadata
import uniffi.xmtpv3.FfiDeliveryStatus
import uniffi.xmtpv3.FfiDirection
import uniffi.xmtpv3.FfiListMessagesOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMessageDisappearingSettings
import uniffi.xmtpv3.FfiSortBy
import uniffi.xmtpv3.FfiSubscribeException
import java.util.Date

class Dm(
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

    val peerInboxId: InboxId
        get() = libXMTPGroup.dmPeerInboxId() ?: throw XMTPException("peerInboxId not found")

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

    private suspend fun metadata(): FfiConversationMetadata =
        withContext(Dispatchers.IO) { libXMTPGroup.groupMetadata() }

    suspend fun send(text: String): String =
        withContext(Dispatchers.IO) {
            val (encodedContent, opts) = encodeContent(content = text, options = null)
            send(encodedContent, opts)
        }

    suspend fun <T> send(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            val (encodedContent, opts) = encodeContent(content = content, options = options)
            send(encodedContent, opts)
        }

    suspend fun send(
        encodedContent: EncodedContent,
        opts: MessageVisibilityOptions = MessageVisibilityOptions(shouldPush = true),
    ): String =
        withContext(Dispatchers.IO) {
            val messageId =
                libXMTPGroup.send(
                    contentBytes = encodedContent.toByteArray(),
                    opts = opts.toFfi(),
                )
            messageId.toHex()
        }

    fun <T> encodeContent(
        content: T,
        options: SendOptions?,
    ): Pair<EncodedContent, MessageVisibilityOptions> {
        val codec = Client.codecRegistry.find(options?.contentType)

        fun <Codec : ContentCodec<T>> encode(
            codec: Codec,
            content: T,
        ): EncodedContent = codec.encode(content)
        try {
            @Suppress("UNCHECKED_CAST")
            val typedCodec = codec as ContentCodec<T>
            var encoded = encode(typedCodec, content)
            val fallback = codec.fallback(content)
            if (!fallback.isNullOrBlank()) {
                encoded = encoded.toBuilder().also { it.fallback = fallback }.build()
            }
            val compression = options?.compression
            if (compression != null) {
                encoded = encoded.compress(compression)
            }
            val sendOpts = MessageVisibilityOptions(shouldPush = typedCodec.shouldPush(content))
            return Pair(encoded, sendOpts)
        } catch (e: Exception) {
            throw XMTPException("Codec type is not registered")
        }
    }

    suspend fun prepareMessage(
        encodedContent: EncodedContent,
        opts: MessageVisibilityOptions = MessageVisibilityOptions(shouldPush = true),
    ): String =
        withContext(Dispatchers.IO) {
            libXMTPGroup.sendOptimistic(encodedContent.toByteArray(), opts.toFfi()).toHex()
        }

    suspend fun <T> prepareMessage(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            val (encodedContent, opts) = encodeContent(content = content, options = options)
            libXMTPGroup.sendOptimistic(encodedContent.toByteArray(), opts.toFfi()).toHex()
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
        excludeContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
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
                            excludeContentTypes = excludeContentTypes,
                            excludeSenderInboxIds = excludeSenderInboxIds,
                            insertedAfterNs = insertedAfterNs,
                            insertedBeforeNs = insertedBeforeNs,
                            sortBy =
                                when (sortBy) {
                                    SortBy.SENT_TIME -> FfiSortBy.SENT_AT
                                    SortBy.INSERTED_TIME -> FfiSortBy.INSERTED_AT
                                },
                        ),
                ).mapNotNull { DecodedMessage.create(it) }
        }

    suspend fun countMessages(
        beforeNs: Long? = null,
        afterNs: Long? = null,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
        excludeContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
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
                        excludeContentTypes = excludeContentTypes,
                        excludeSenderInboxIds = excludeSenderInboxIds,
                        insertedAfterNs = insertedAfterNs,
                        insertedBeforeNs = insertedBeforeNs,
                        sortBy = null,
                    ),
            )
        }

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: SortDirection = SortDirection.DESCENDING,
        deliveryStatus: MessageDeliveryStatus = MessageDeliveryStatus.ALL,
        excludeContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
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
                            direction =
                                when (direction) {
                                    SortDirection.ASCENDING ->
                                        FfiDirection.ASCENDING

                                    else -> FfiDirection.DESCENDING
                                },
                            contentTypes = null,
                            excludeContentTypes = excludeContentTypes,
                            excludeSenderInboxIds = excludeSenderInboxIds,
                            insertedAfterNs = insertedAfterNs,
                            insertedBeforeNs = insertedBeforeNs,
                            sortBy =
                                when (sortBy) {
                                    SortBy.SENT_TIME -> FfiSortBy.SENT_AT
                                    SortBy.INSERTED_TIME -> FfiSortBy.INSERTED_AT
                                },
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
        excludeContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
    ): List<DecodedMessageV2> =
        withContext(Dispatchers.IO) {
            libXMTPGroup
                .findEnrichedMessages(
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
                            excludeContentTypes = excludeContentTypes,
                            excludeSenderInboxIds = excludeSenderInboxIds,
                            insertedAfterNs = insertedAfterNs,
                            insertedBeforeNs = insertedBeforeNs,
                            sortBy =
                                when (sortBy) {
                                    SortBy.SENT_TIME -> FfiSortBy.SENT_AT
                                    SortBy.INSERTED_TIME -> FfiSortBy.INSERTED_AT
                                },
                        ),
                ).mapNotNull { DecodedMessageV2.create(it) }
        }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? =
        withContext(Dispatchers.IO) {
            val message = libXMTPGroup.processStreamedConversationMessage(messageBytes)
            DecodedMessage.create(message)
        }

    suspend fun creatorInboxId(): InboxId = withContext(Dispatchers.IO) { metadata().creatorInboxId() }

    suspend fun isCreator(): Boolean = withContext(Dispatchers.IO) { metadata().creatorInboxId() == client.inboxId }

    suspend fun isActive(): Boolean = withContext(Dispatchers.IO) { libXMTPGroup.isActive() }

    suspend fun members(): List<Member> = withContext(Dispatchers.IO) { libXMTPGroup.listMembers().map { Member(it) } }

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
                                    "XMTP Dm stream",
                                    "Failed to decode message: id=${message.id.toHex()}, " +
                                        "conversationId=${message.conversationId.toHex()}, " +
                                        "senderInboxId=${message.senderInboxId}",
                                )
                            }
                        } catch (e: Exception) {
                            Log.e(
                                "XMTP Dm stream",
                                "Error decoding message: id=${message.id.toHex()}, " +
                                    "conversationId=${message.conversationId.toHex()}, " +
                                    "senderInboxId=${message.senderInboxId}",
                                e,
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

    suspend fun updateConsentState(state: ConsentState) =
        withContext(Dispatchers.IO) {
            val consentState = ConsentState.toFfiConsentState(state)
            libXMTPGroup.updateConsentState(consentState)
        }

    suspend fun consentState(): ConsentState =
        withContext(Dispatchers.IO) {
            ConsentState.fromFfiConsentState(libXMTPGroup.consentState())
        }

    // Returns null if dm is not paused, otherwise the min version required to unpause this dm
    suspend fun pausedForVersion(): String? = withContext(Dispatchers.IO) { libXMTPGroup.pausedForVersion() }

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

    suspend fun getPushTopics(): List<String> =
        withContext(Dispatchers.IO) {
            val duplicates = libXMTPGroup.findDuplicateDms()
            val topicIds = duplicates.map { it.id().toHex() }.toMutableList()
            topicIds.add(id)
            topicIds.map { Topic.groupMessage(it).description }
        }

    suspend fun getDebugInformation(): ConversationDebugInfo =
        withContext(Dispatchers.IO) {
            ConversationDebugInfo(libXMTPGroup.conversationDebugInfo())
        }

    suspend fun getLastReadTimes(): Map<InboxId, Long> = withContext(Dispatchers.IO) { libXMTPGroup.getLastReadTimes() }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (javaClass != other?.javaClass) return false

        other as Dm

        return id == other.id
    }

    override fun hashCode(): Int = id.hashCode()
}
