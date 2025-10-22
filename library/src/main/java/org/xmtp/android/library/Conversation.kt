package org.xmtp.android.library

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.libxmtp.ConversationDebugInfo
import org.xmtp.android.library.libxmtp.DecodedMessage
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

    suspend fun <T> prepareMessage(
        content: T,
        options: SendOptions? = null,
    ): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.prepareMessage(content, options)
                is Dm -> dm.prepareMessage(content, options)
            }
        }

    suspend fun prepareMessage(encodedContent: EncodedContent): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.prepareMessage(encodedContent)
                is Dm -> dm.prepareMessage(encodedContent)
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

    suspend fun send(encodedContent: EncodedContent): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.send(encodedContent)
                is Dm -> dm.send(encodedContent)
            }
        }

    suspend fun send(text: String): String =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.send(text)
                is Dm -> dm.send(text)
            }
        }

    suspend fun sync() =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.sync()
                is Dm -> dm.sync()
            }
        }

    suspend fun messages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
    ): List<DecodedMessage> =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.messages(limit, beforeNs, afterNs, direction, deliveryStatus, excludedContentTypes)
                is Dm -> dm.messages(limit, beforeNs, afterNs, direction, deliveryStatus, excludedContentTypes)
            }
        }

    suspend fun countMessages(
        beforeNs: Long? = null,
        afterNs: Long? = null,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
    ): Long =
        withContext(Dispatchers.IO) {
            when (this@Conversation) {
                is Group -> group.countMessages(beforeNs, afterNs, deliveryStatus, excludedContentTypes)
                is Dm -> dm.countMessages(beforeNs, afterNs, deliveryStatus, excludedContentTypes)
            }
        }

    suspend fun enrichedMessages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus =
            DecodedMessage.MessageDeliveryStatus.ALL,
        excludedContentTypes: List<FfiContentType>? = null,
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
                    )

                is Dm ->
                    dm.enrichedMessages(limit, beforeNs, afterNs, direction, deliveryStatus, excludedContentTypes)
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
                    )

                is Dm ->
                    dm.messagesWithReactions(
                        limit,
                        beforeNs,
                        afterNs,
                        direction,
                        deliveryStatus,
                        excludedContentTypes,
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
