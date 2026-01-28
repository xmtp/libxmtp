package org.xmtp.android.library

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.libxmtp.ConversationDebugInfo
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DecodedMessage.SortBy
import org.xmtp.android.library.libxmtp.DecodedMessageV2
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.proto.keystore.api.v1.Keystore
import uniffi.xmtpv3.FfiContentType
import java.util.Date

sealed class Conversation {
    data class Group(
        val group: org.xmtp.android.library.Group,
    ) : Conversation()

    data class Dm(
        val dm: org.xmtp.android.library.Dm,
    ) : Conversation()

    enum class Type {
        GROUP,
        DM,
    }

    val type: Type
        get() {
            return when (this) {
                is Group -> Type.GROUP
                is Dm -> Type.DM
            }
        }

    val id: String
        get() {
            return when (this) {
                is Group -> group.id
                is Dm -> dm.id
            }
        }

    val topic: String
        get() {
            return when (this) {
                is Group -> group.topic
                is Dm -> dm.topic
            }
        }

    val createdAt: Date
        get() {
            return when (this) {
                is Group -> group.createdAt
                is Dm -> dm.createdAt
            }
        }

    val createdAtNs: Long
        get() {
            return when (this) {
                is Group -> group.createdAtNs
                is Dm -> dm.createdAtNs
            }
        }

    val lastActivityNs: Long
        get() {
            return when (this) {
                is Group -> group.lastActivityNs
                is Dm -> dm.lastActivityNs
            }
        }

    @Deprecated(
        message = "Use suspend disappearingMessageSettings()",
        replaceWith = ReplaceWith("disappearingMessageSettings()"),
    )
    val disappearingMessageSettings: DisappearingMessageSettings?
        get() {
            return when (this) {
                is Group -> runBlocking { group.disappearingMessageSettings() }
                is Dm -> runBlocking { dm.disappearingMessageSettings() }
            }
        }

    suspend fun disappearingMessageSettings(): DisappearingMessageSettings? =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.disappearingMessageSettings()
                is Dm -> dm.disappearingMessageSettings()
            }
        }

    @Deprecated(
        message = "Use suspend isDisappearingMessagesEnabled()",
        replaceWith = ReplaceWith("isDisappearingMessagesEnabled()"),
    )
    val isDisappearingMessagesEnabled: Boolean
        get() {
            return when (this) {
                is Group -> runBlocking { group.isDisappearingMessagesEnabled() }
                is Dm -> runBlocking { dm.isDisappearingMessagesEnabled() }
            }
        }

    suspend fun isDisappearingMessagesEnabled(): Boolean =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.isDisappearingMessagesEnabled()
                is Dm -> dm.isDisappearingMessagesEnabled()
            }
        }

    suspend fun lastMessage(): DecodedMessage? =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.lastMessage()
                is Dm -> dm.lastMessage()
            }
        }

    fun commitLogForkStatus(): ConversationDebugInfo.CommitLogForkStatus =
        when (this) {
            is Group -> group.commitLogForkStatus()
            is Dm -> dm.commitLogForkStatus()
        }

    suspend fun members(): List<Member> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.members()
                is Dm -> dm.members()
            }
        }

    suspend fun clearDisappearingMessageSettings() =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.clearDisappearingMessageSettings()
                is Dm -> dm.clearDisappearingMessageSettings()
            }
        }

    suspend fun updateDisappearingMessageSettings(disappearingMessageSettings: DisappearingMessageSettings?) =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.updateDisappearingMessageSettings(disappearingMessageSettings)
                is Dm -> dm.updateDisappearingMessageSettings(disappearingMessageSettings)
            }
        }

    suspend fun updateConsentState(state: ConsentState) =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.updateConsentState(state)
                is Dm -> dm.updateConsentState(state)
            }
        }

    suspend fun consentState(): ConsentState =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.consentState()
                is Dm -> dm.consentState()
            }
        }

    /**
     * Prepares a message for sending.
     * @param noSend When true, the prepared message will not be published until
     *               [publishMessage] is called with the returned message ID.
     *               When false (default), uses optimistic sending and the message
     *               will be published with the next [publishMessages] call.
     */
    suspend fun <T> prepareMessage(
        content: T,
        options: SendOptions? = null,
        noSend: Boolean = false,
    ): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.prepareMessage(content, options, noSend)
                is Dm -> dm.prepareMessage(content, options, noSend)
            }
        }

    /**
     * Prepares a message for sending.
     * @param noSend When true, the prepared message will not be published until
     *               [publishMessage] is called with the returned message ID.
     *               When false (default), uses optimistic sending and the message
     *               will be published with the next [publishMessages] call.
     */
    suspend fun prepareMessage(
        encodedContent: EncodedContent,
        opts: MessageVisibilityOptions = MessageVisibilityOptions(shouldPush = true),
        noSend: Boolean = false,
    ): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.prepareMessage(encodedContent, opts, noSend)
                is Dm -> dm.prepareMessage(encodedContent, opts, noSend)
            }
        }

    suspend fun <T> send(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.send(content = content, options = options)
                is Dm -> dm.send(content = content, options = options)
            }
        }

    suspend fun send(
        encodedContent: EncodedContent,
        opts: MessageVisibilityOptions = MessageVisibilityOptions(shouldPush = true),
    ): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.send(encodedContent, opts)
                is Dm -> dm.send(encodedContent, opts)
            }
        }

    suspend fun send(text: String): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.send(text)
                is Dm -> dm.send(text)
            }
        }

    /**
     * Delete a message by its ID.
     *
     * Users can delete their own messages. In groups, super admins can delete any message.
     *
     * @param messageId The hex-encoded ID of the message to delete.
     * @return The hex-encoded ID of the deletion message.
     * @throws XMTPException if deletion fails (e.g., message not found, not authorized, already deleted).
     */
    suspend fun deleteMessage(messageId: String): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.deleteMessage(messageId)
                is Dm -> dm.deleteMessage(messageId)
            }
        }

    suspend fun sync() =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.sync()
                is Dm -> dm.sync()
            }
        }

    /**
     * Get the raw list of messages from a conversation.
     *
     * This method returns all messages in chronological order without additional processing.
     * Reactions, replies, and other associated metadata are returned as separate messages
     * and are not linked to their parent messages.
     *
     * For UI rendering, consider using [enrichedMessages] instead, which provides messages
     * with enriched metadata automatically included.
     *
     * @see enrichedMessages
     */
    suspend fun messages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
    ): List<DecodedMessage> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group ->
                    group.messages(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )
                is Dm ->
                    dm.messages(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )
            }
        }

    suspend fun countMessages(
        beforeNs: Long? = null,
        afterNs: Long? = null,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
    ): Long =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group ->
                    group.countMessages(
                        beforeNs,
                        afterNs,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                    )
                is Dm ->
                    dm.countMessages(
                        beforeNs,
                        afterNs,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                    )
            }
        }

    /**
     * Get messages with enriched metadata automatically included.
     *
     * This method retrieves messages with reactions, replies, and other associated data
     * "baked in" to each message, eliminating the need for separate queries to fetch
     * this information.
     *
     * **Recommended for UI rendering.** This method provides better performance and
     * simpler code compared to [messages] when displaying conversations.
     *
     * When handling content types, use the generic `content<T>()` method with the
     * appropriate type for reactions and replies.
     *
     * @return List of [DecodedMessageV2] with enriched metadata.
     * @see messages
     */
    suspend fun enrichedMessages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
    ): List<DecodedMessageV2> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group ->
                    group.enrichedMessages(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )

                is Dm ->
                    dm.enrichedMessages(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )
            }
        }

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
        excludeSenderInboxIds: List<String>? = null,
        insertedAfterNs: Long? = null,
        insertedBeforeNs: Long? = null,
        sortBy: SortBy = SortBy.SENT_TIME,
    ): List<DecodedMessage> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group ->
                    group.messagesWithReactions(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )

                is Dm ->
                    dm.messagesWithReactions(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
                        excludeSenderInboxIds,
                        insertedAfterNs,
                        insertedBeforeNs,
                        sortBy,
                    )
            }
        }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.processMessage(messageBytes)
                is Dm -> dm.processMessage(messageBytes)
            }
        }

    suspend fun publishMessages() =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.publishMessages()
                is Dm -> dm.publishMessages()
            }
        }

    /**
     * Publishes a message that was prepared with noSend = true.
     * @param id The message ID returned from [prepareMessage] when called with noSend = true
     */
    suspend fun publishMessage(id: String) =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.publishMessage(id)
                is Dm -> dm.publishMessage(id)
            }
        }

    // Returns null if conversation is not paused, otherwise the min version required to unpause
    // this conversation
    suspend fun pausedForVersion(): String? =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.pausedForVersion()
                is Dm -> dm.pausedForVersion()
            }
        }

    val client: Client
        get() {
            return when (this) {
                is Group -> group.client
                is Dm -> dm.client
            }
        }

    fun streamMessages(onClose: (() -> Unit)? = null): Flow<DecodedMessage> =
        when (this) {
            is Group -> group.streamMessages(onClose)
            is Dm -> dm.streamMessages(onClose)
        }

    suspend fun getHmacKeys(): Keystore.GetConversationHmacKeysResponse =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.getHmacKeys()
                is Dm -> dm.getHmacKeys()
            }
        }

    suspend fun getPushTopics(): List<String> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.getPushTopics()
                is Dm -> dm.getPushTopics()
            }
        }

    suspend fun getDebugInformation(): ConversationDebugInfo =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.getDebugInformation()
                is Dm -> dm.getDebugInformation()
            }
        }

    suspend fun isActive(): Boolean =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.isActive()
                is Dm -> dm.isActive()
            }
        }

    // Get the last read receipt timestamp (in nanoseconds) for each member of the conversation,
    // keyed by inbox ID
    suspend fun getLastReadTimes(): Map<InboxId, Long> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.getLastReadTimes()
                is Dm -> dm.getLastReadTimes()
            }
        }
}
