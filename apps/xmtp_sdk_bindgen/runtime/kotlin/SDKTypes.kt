package uniffi.xmtp_sdk

import java.lang.ref.WeakReference
import java.time.Instant
import java.util.concurrent.ConcurrentHashMap

private fun validHex(
    value: String,
    bytes: Int,
): Boolean = value.length == bytes * 2 && value.all { it in '0'..'9' || it in 'a'..'f' }

data class InboxID(
    val value: String,
) {
    override fun toString() = value

    companion object {
        fun fromString(value: String): InboxID {
            require(value.isNotEmpty()) { "inbox ID is empty" }
            return InboxID(value)
        }
    }
}

data class InstallationID(
    val value: String,
) {
    override fun toString() = value

    companion object {
        fun fromString(value: String): InstallationID {
            require(validHex(value, 32)) { "invalid lowercase hex ID" }
            return InstallationID(value)
        }
    }
}

data class ConversationID(
    val value: String,
) {
    override fun toString() = value

    companion object {
        fun fromString(value: String): ConversationID {
            require(validHex(value, 16)) { "invalid lowercase hex ID" }
            return ConversationID(value)
        }
    }
}

data class MessageID(
    val value: String,
) {
    override fun toString() = value

    companion object {
        fun fromString(value: String): MessageID {
            require(validHex(value, 32)) { "invalid lowercase hex ID" }
            return MessageID(value)
        }
    }
}

data class Timestamp(
    val ns: Long,
) {
    val date: Instant get() =
        Instant.ofEpochSecond(
            Math.floorDiv(ns, 1_000_000_000L),
            Math.floorMod(ns, 1_000_000_000L),
        )
}

class Message(
    val data: MessageData,
) {
    val id get() = data.id
    val conversationID get() = data.conversationID
    val senderInboxID get() = data.senderInboxID
    val sentAt get() = data.sentAt
    val kind get() = data.kind
    val deliveryStatus get() = data.deliveryStatus
    val contentType get() = data.contentType
    val fallback get() = data.fallback
    val content get() = data.content

    fun client(): Client =
        ClientRegistry.get(data.clientKey)
            ?: throw IllegalStateException("clientClosed")

    override fun equals(other: Any?): Boolean =
        other is Message &&
            id == other.id && data.conversationID == other.data.conversationID &&
            data.senderInboxID == other.data.senderInboxID && data.sentAt == other.data.sentAt &&
            data.kind == other.data.kind && data.deliveryStatus == other.data.deliveryStatus &&
            data.contentType == other.data.contentType && data.fallback == other.data.fallback &&
            when (val value = data.content) {
                is MessageContent.Text -> {
                    value == other.data.content
                }

                is MessageContent.Unknown -> {
                    val otherContent = other.data.content
                    otherContent is MessageContent.Unknown &&
                        value.encoded.contentEquals(otherContent.encoded)
                }
            }

    override fun hashCode(): Int {
        var result = id.hashCode()
        result = 31 * result + data.conversationID.hashCode()
        result = 31 * result + data.senderInboxID.hashCode()
        result = 31 * result + data.sentAt.hashCode()
        result = 31 * result + data.kind.hashCode()
        result = 31 * result + data.deliveryStatus.hashCode()
        result = 31 * result + data.contentType.hashCode()
        result = 31 * result + (data.fallback?.hashCode() ?: 0)
        result = 31 * result +
            when (val value = data.content) {
                is MessageContent.Text -> value.hashCode()
                is MessageContent.Unknown -> value.encoded.contentHashCode()
            }
        return result
    }
}

object ClientRegistry {
    private val entries = ConcurrentHashMap<ULong, WeakReference<Client>>()

    fun register(client: Client) {
        entries[client.clientKey()] = WeakReference(client)
    }

    fun get(key: ULong): Client? = entries[key]?.get()

    fun remove(client: Client) {
        entries.remove(client.clientKey())
    }
}
