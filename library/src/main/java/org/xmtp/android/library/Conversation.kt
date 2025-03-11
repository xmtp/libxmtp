package org.xmtp.android.library

import kotlinx.coroutines.flow.Flow
import org.xmtp.android.library.codecs.EncodedContent
import org.xmtp.android.library.libxmtp.Member
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import java.util.Date

sealed class Conversation {
    data class Group(val group: org.xmtp.android.library.Group) : Conversation()
    data class Dm(val dm: org.xmtp.android.library.Dm) : Conversation()

    enum class Type { GROUP, DM }

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

    val disappearingMessageSettings: DisappearingMessageSettings?
        get() {
            return when (this) {
                is Group -> group.disappearingMessageSettings
                is Dm -> dm.disappearingMessageSettings
            }
        }

    val isDisappearingMessagesEnabled: Boolean
        get() {
            return when (this) {
                is Group -> group.isDisappearingMessagesEnabled
                is Dm -> dm.isDisappearingMessagesEnabled
            }
        }

    suspend fun lastMessage(): DecodedMessage? {
        return when (this) {
            is Group -> group.lastMessage()
            is Dm -> dm.lastMessage()
        }
    }

    suspend fun members(): List<Member> {
        return when (this) {
            is Group -> group.members()
            is Dm -> dm.members()
        }
    }

    suspend fun clearDisappearingMessageSettings() {
        return when (this) {
            is Group -> group.clearDisappearingMessageSettings()
            is Dm -> dm.clearDisappearingMessageSettings()
        }
    }

    suspend fun updateDisappearingMessageSettings(disappearingMessageSettings: DisappearingMessageSettings?) {
        return when (this) {
            is Group -> group.updateDisappearingMessageSettings(disappearingMessageSettings)
            is Dm -> dm.updateDisappearingMessageSettings(disappearingMessageSettings)
        }
    }

    fun updateConsentState(state: ConsentState) {
        return when (this) {
            is Group -> group.updateConsentState(state)
            is Dm -> dm.updateConsentState(state)
        }
    }

    fun consentState(): ConsentState {
        return when (this) {
            is Group -> group.consentState()
            is Dm -> dm.consentState()
        }
    }

    fun <T> prepareMessage(content: T, options: SendOptions? = null): String {
        return when (this) {
            is Group -> group.prepareMessage(content, options)
            is Dm -> dm.prepareMessage(content, options)
        }
    }

    fun prepareMessage(encodedContent: EncodedContent): String {
        return when (this) {
            is Group -> group.prepareMessage(encodedContent)
            is Dm -> dm.prepareMessage(encodedContent)
        }
    }

    suspend fun <T> send(content: T, options: SendOptions? = null): String {
        return when (this) {
            is Group -> group.send(content = content, options = options)
            is Dm -> dm.send(content = content, options = options)
        }
    }

    suspend fun send(encodedContent: EncodedContent): String {
        return when (this) {
            is Group -> group.send(encodedContent)
            is Dm -> dm.send(encodedContent)
        }
    }

    suspend fun send(text: String): String {
        return when (this) {
            is Group -> group.send(text)
            is Dm -> dm.send(text)
        }
    }

    suspend fun sync() {
        return when (this) {
            is Group -> group.sync()
            is Dm -> dm.sync()
        }
    }

    suspend fun messages(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus = DecodedMessage.MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        return when (this) {
            is Group -> group.messages(limit, beforeNs, afterNs, direction, deliveryStatus)
            is Dm -> dm.messages(limit, beforeNs, afterNs, direction, deliveryStatus)
        }
    }

    suspend fun messagesWithReactions(
        limit: Int? = null,
        beforeNs: Long? = null,
        afterNs: Long? = null,
        direction: DecodedMessage.SortDirection = DecodedMessage.SortDirection.DESCENDING,
        deliveryStatus: DecodedMessage.MessageDeliveryStatus = DecodedMessage.MessageDeliveryStatus.ALL,
    ): List<DecodedMessage> {
        return when (this) {
            is Group -> group.messagesWithReactions(
                limit,
                beforeNs,
                afterNs,
                direction,
                deliveryStatus
            )

            is Dm -> dm.messagesWithReactions(limit, beforeNs, afterNs, direction, deliveryStatus)
        }
    }

    suspend fun processMessage(messageBytes: ByteArray): DecodedMessage? {
        return when (this) {
            is Group -> group.processMessage(messageBytes)
            is Dm -> dm.processMessage(messageBytes)
        }
    }

    suspend fun publishMessages() {
        return when (this) {
            is Group -> group.publishMessages()
            is Dm -> dm.publishMessages()
        }
    }

    // Returns null if conversation is not paused, otherwise the min version required to unpause this conversation
    fun pausedForVersion(): String? {
        return when (this) {
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

    fun streamMessages(): Flow<DecodedMessage> {
        return when (this) {
            is Group -> group.streamMessages()
            is Dm -> dm.streamMessages()
        }
    }
}
