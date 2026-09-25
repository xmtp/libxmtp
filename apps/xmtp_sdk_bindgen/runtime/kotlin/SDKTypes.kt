package uniffi.xmtp_sdk

import java.lang.ref.WeakReference
import java.time.Instant
import java.util.concurrent.ConcurrentHashMap

interface SDKContentCodec {
    val type: ContentTypeID

    fun encode(value: Any): EncodedContent

    fun decode(encoded: EncodedContent): Any

    val key: SDKContentCodecKey get() = SDKContentCodecKey(type)
}

data class SDKContentCodecKey(
    val authorityID: String,
    val typeID: String,
    val versionMajor: UInt,
) {
    constructor(type: ContentTypeID) : this(type.authorityID, type.typeID, type.versionMajor)
}

sealed class SDKMessageContent {
    data class Standard(
        val value: MessageContent,
    ) : SDKMessageContent()

    data class Custom(
        val encoded: EncodedContent,
        val value: Any?,
        val error: Throwable?,
    ) : SDKMessageContent()

    data class Unknown(
        val encoded: EncodedContent,
    ) : SDKMessageContent()
}

sealed class SDKReplyContent {
    data class Standard(
        val value: MessageBody,
    ) : SDKReplyContent()

    data class Custom(
        val encoded: EncodedContent,
        val value: Any?,
        val error: Throwable?,
    ) : SDKReplyContent()

    data class Unknown(
        val encoded: EncodedContent,
    ) : SDKReplyContent()
}

private fun validHex(
    value: String,
    bytes: Int,
): Boolean = value.length == bytes * 2 && value.all { it in '0'..'9' || it in 'a'..'f' }

private fun EncodedContent.deepEquals(other: EncodedContent): Boolean =
    type == other.type && parameters == other.parameters && fallback == other.fallback &&
        content.contentEquals(other.content)

private fun EncodedContent.deepHashCode(): Int {
    var result = type.hashCode()
    result = 31 * result + parameters.hashCode()
    result = 31 * result + (fallback?.hashCode() ?: 0)
    return 31 * result + content.contentHashCode()
}

private fun invalidID(message: String) =
    XmtpException.InvalidArgument(
        ErrorDetails("InvalidArgument", ErrorCategory.INPUT, false, message),
    )

// ID types have no public constructor or copy(), so a caller can only make
// one through fromString. Generated lifts use the internal unchecked factory.

class InboxID private constructor(
    val value: String,
) {
    override fun toString() = value

    override fun equals(other: Any?) = other is InboxID && other.value == value

    override fun hashCode() = value.hashCode()

    companion object {
        fun fromString(value: String): InboxID {
            if (value.isEmpty()) throw invalidID("inbox ID is empty")
            return InboxID(value)
        }

        internal fun unchecked(value: String) = InboxID(value)
    }
}

class InstallationID private constructor(
    val value: String,
) {
    override fun toString() = value

    override fun equals(other: Any?) = other is InstallationID && other.value == value

    override fun hashCode() = value.hashCode()

    companion object {
        fun fromString(value: String): InstallationID {
            if (!validHex(value, 32)) throw invalidID("invalid lowercase hex ID")
            return InstallationID(value)
        }

        internal fun unchecked(value: String) = InstallationID(value)
    }
}

class ConversationID private constructor(
    val value: String,
) {
    override fun toString() = value

    override fun equals(other: Any?) = other is ConversationID && other.value == value

    override fun hashCode() = value.hashCode()

    companion object {
        fun fromString(value: String): ConversationID {
            if (!validHex(value, 16)) throw invalidID("invalid lowercase hex ID")
            return ConversationID(value)
        }

        internal fun unchecked(value: String) = ConversationID(value)
    }
}

class MessageID private constructor(
    val value: String,
) {
    override fun toString() = value

    override fun equals(other: Any?) = other is MessageID && other.value == value

    override fun hashCode() = value.hashCode()

    companion object {
        fun fromString(value: String): MessageID {
            if (!validHex(value, 32)) throw invalidID("invalid lowercase hex ID")
            return MessageID(value)
        }

        internal fun unchecked(value: String) = MessageID(value)
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

private fun decodeReplyBody(
    body: MessageBody,
    clientKey: ULong,
): SDKReplyContent =
    when (body) {
        is MessageBody.Custom -> {
            when (val decoded = ClientRegistry.get(clientKey)?.decodeCustom(body.encoded)) {
                is SDKMessageContent.Custom -> SDKReplyContent.Custom(body.encoded, decoded.value, decoded.error)
                is SDKMessageContent.Unknown -> SDKReplyContent.Unknown(body.encoded)
                else -> SDKReplyContent.Custom(body.encoded, null, clientClosedError())
            }
        }

        is MessageBody.Unknown -> {
            SDKReplyContent.Unknown(body.encoded)
        }

        else -> {
            SDKReplyContent.Standard(body)
        }
    }

class Message(
    val data: MessageData,
) {
    val content: SDKMessageContent =
        when (val body = data.content) {
            is MessageContent.Custom -> {
                ClientRegistry.get(data.clientKey)?.decodeCustom(body.encoded)
                    ?: SDKMessageContent.Custom(
                        body.encoded,
                        null,
                        XmtpException.ClientClosed(
                            ErrorDetails("ClientClosed", ErrorCategory.LIFECYCLE, false, "client is closed"),
                        ),
                    )
            }

            else -> {
                SDKMessageContent.Standard(body)
            }
        }
    val inReplyToContent: SDKReplyContent? =
        data.inReplyTo?.let { decodeReplyBody(it.content, data.clientKey) }
    val replyContent: SDKReplyContent? =
        (data.content as? MessageContent.Reply)?.let { decodeReplyBody(it.body, data.clientKey) }
    val id get() = data.id
    val conversationID get() = data.conversationID
    val topic get() = data.topic
    val senderInboxID get() = data.senderInboxID
    val sentAt get() = data.sentAt
    val kind get() = data.kind
    val deliveryStatus get() = data.deliveryStatus
    val contentType get() = data.contentType
    val fallback get() = data.fallback
    val encoded get() = data.encoded
    val replyCount get() = data.replyCount
    val reactions get() = data.reactions
    val inReplyTo get() = data.inReplyTo
    val insertedAt get() = data.insertedAt
    val expiresAt get() = data.expiresAt

    suspend fun refresh(): Message? = client().raw.conversations().getMessageByID(id)

    suspend fun delete(): MessageID = client().raw.conversations().deleteMessage(id)

    suspend fun deleteLocally() = client().raw.conversations().deleteMessageLocally(id)

    suspend fun react(
        reaction: Reaction,
        options: SendOptions? = null,
    ): MessageID = client().raw.conversations().reactToMessage(id, reaction, options)

    suspend fun reply(
        text: String,
        options: SendOptions? = null,
    ): MessageID = client().raw.conversations().replyToMessage(id, encodeText(text), options)

    suspend fun reply(
        content: EncodedContent,
        options: SendOptions? = null,
    ): MessageID = client().raw.conversations().replyToMessage(id, content, options)

    suspend fun reply(
        codec: SDKContentCodec,
        value: Any,
        options: SendOptions? = null,
    ): MessageID = reply(codec.encode(value), options)

    suspend fun parent(): Message? = inReplyTo?.id?.let { client().raw.conversations().getMessageByID(it) }

    suspend fun conversation(): Conversation? = client().raw.conversations().getByID(conversationID)

    fun client(): SDKClient =
        ClientRegistry.get(data.clientKey)
            ?: throw clientClosedError()

    override fun equals(other: Any?): Boolean =
        other is Message &&
            id == other.id && data.clientKey == other.data.clientKey &&
            data.conversationID == other.data.conversationID && data.topic == other.data.topic &&
            data.senderInboxID == other.data.senderInboxID && data.sentAt == other.data.sentAt &&
            data.kind == other.data.kind && data.deliveryStatus == other.data.deliveryStatus &&
            data.contentType == other.data.contentType && data.fallback == other.data.fallback &&
            data.insertedAt == other.data.insertedAt && data.expiresAt == other.data.expiresAt &&
            data.replyCount == other.data.replyCount && data.reactions == other.data.reactions &&
            data.inReplyTo.deepEquals(other.data.inReplyTo) &&
            data.encoded.deepEquals(other.data.encoded) &&
            when (val value = data.content) {
                is MessageContent.Text -> {
                    value == other.data.content
                }

                is MessageContent.Markdown -> {
                    value == other.data.content
                }

                is MessageContent.ReadReceipt -> {
                    other.data.content is MessageContent.ReadReceipt
                }

                is MessageContent.Reaction -> {
                    value == other.data.content
                }

                is MessageContent.Reply -> {
                    value == other.data.content
                }

                is MessageContent.Custom -> {
                    val otherContent = other.data.content
                    otherContent is MessageContent.Custom &&
                        value.encoded.deepEquals(otherContent.encoded) &&
                        value.rawBytes.contentEquals(otherContent.rawBytes)
                }

                is MessageContent.Unknown -> {
                    val otherContent = other.data.content
                    otherContent is MessageContent.Unknown &&
                        value.encoded.deepEquals(otherContent.encoded) &&
                        value.rawBytes.contentEquals(otherContent.rawBytes)
                }

                else -> {
                    value == other.data.content
                }
            }

    override fun hashCode(): Int {
        var result = id.hashCode()
        result = 31 * result + data.clientKey.hashCode()
        result = 31 * result + data.conversationID.hashCode()
        result = 31 * result + data.senderInboxID.hashCode()
        result = 31 * result + data.sentAt.hashCode()
        result = 31 * result + data.kind.hashCode()
        result = 31 * result + data.deliveryStatus.hashCode()
        result = 31 * result + data.contentType.hashCode()
        result = 31 * result + (data.fallback?.hashCode() ?: 0)
        result = 31 * result + data.insertedAt.hashCode()
        result = 31 * result + (data.expiresAt?.hashCode() ?: 0)
        result = 31 * result + data.replyCount.hashCode()
        result = 31 * result + data.reactions.hashCode()
        result = 31 * result + data.inReplyTo.deepHashCode()
        result = 31 * result + data.encoded.deepHashCode()
        result = 31 * result +
            when (val value = data.content) {
                is MessageContent.Text -> {
                    value.hashCode()
                }

                is MessageContent.Markdown -> {
                    value.hashCode()
                }

                is MessageContent.ReadReceipt -> {
                    0
                }

                is MessageContent.Reaction -> {
                    value.hashCode()
                }

                is MessageContent.Reply -> {
                    value.hashCode()
                }

                is MessageContent.Custom -> {
                    31 * value.encoded.deepHashCode() + value.rawBytes.contentHashCode()
                }

                is MessageContent.Unknown -> {
                    31 * value.encoded.deepHashCode() + value.rawBytes.contentHashCode()
                }

                else -> {
                    value.hashCode()
                }
            }
        return result
    }
}

private fun clientClosedError() =
    XmtpException.ClientClosed(
        ErrorDetails("ClientClosed", ErrorCategory.LIFECYCLE, false, "client is closed"),
    )

private fun MessageBody.deepEquals(other: MessageBody): Boolean =
    when {
        this is MessageBody.Custom && other is MessageBody.Custom -> encoded.deepEquals(other.encoded)
        this is MessageBody.Unknown && other is MessageBody.Unknown -> encoded.deepEquals(other.encoded)
        else -> this == other
    }

private fun MessageBody.deepHashCode(): Int =
    when (this) {
        is MessageBody.Custom -> encoded.deepHashCode()
        is MessageBody.Unknown -> encoded.deepHashCode()
        else -> hashCode()
    }

private fun ReplyParent?.deepEquals(other: ReplyParent?): Boolean =
    when {
        this == null || other == null -> {
            this == null && other == null
        }

        else -> {
            id == other.id && senderInboxID == other.senderInboxID && sentAt == other.sentAt &&
                kind == other.kind && deliveryStatus == other.deliveryStatus &&
                contentType == other.contentType && fallback == other.fallback &&
                content.deepEquals(other.content) &&
                encoded.deepEquals(other.encoded)
        }
    }

private fun ReplyParent?.deepHashCode(): Int {
    if (this == null) return 0
    var result = id.hashCode()
    result = 31 * result + senderInboxID.hashCode()
    result = 31 * result + sentAt.hashCode()
    result = 31 * result + kind.hashCode()
    result = 31 * result + deliveryStatus.hashCode()
    result = 31 * result + contentType.hashCode()
    result = 31 * result + (fallback?.hashCode() ?: 0)
    result = 31 * result + content.deepHashCode()
    return 31 * result + encoded.deepHashCode()
}

object ClientRegistry {
    private val entries = ConcurrentHashMap<ULong, WeakReference<SDKClient>>()

    fun register(client: SDKClient) {
        entries.entries.removeIf { it.value.get() == null }
        entries[client.raw.clientKey()] = WeakReference(client)
    }

    fun get(key: ULong): SDKClient? {
        val client = entries[key]?.get()
        if (client == null) entries.remove(key)
        return client
    }

    fun remove(client: SDKClient) {
        entries.remove(client.raw.clientKey())
    }
}
