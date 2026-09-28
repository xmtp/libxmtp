package uniffi.xmtp_sdk

import java.lang.ref.WeakReference
import java.time.Instant
import java.util.concurrent.ConcurrentHashMap

interface SDKContentCodec {
    val type: ContentTypeId

    fun encode(value: Any): EncodedContent

    fun decode(encoded: EncodedContent): Any

    val key: SDKContentCodecKey get() = SDKContentCodecKey(type)
}

data class SDKContentCodecKey(
    val authorityId: String,
    val typeId: String,
    val versionMajor: UInt,
) {
    constructor(type: ContentTypeId) : this(type.authorityId, type.typeId, type.versionMajor)
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

private fun EncodedContent.deepEquals(other: EncodedContent): Boolean =
    type == other.type && parameters == other.parameters && fallback == other.fallback &&
        content.contentEquals(other.content)

private fun EncodedContent.deepHashCode(): Int {
    var result = type.hashCode()
    result = 31 * result + parameters.hashCode()
    result = 31 * result + (fallback?.hashCode() ?: 0)
    return 31 * result + content.contentHashCode()
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
    val deliveryCursor: String? get() = data.deliveryCursor
    val id get() = data.id
    val conversationId get() = data.conversationId
    val topic get() = data.topic
    val senderInboxId get() = data.senderInboxId
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

    suspend fun refresh(): Message? = client().raw.conversations().getMessageById(id)

    suspend fun delete(): MessageId = client().raw.conversations().deleteMessage(id)

    suspend fun deleteLocally() = client().raw.conversations().deleteMessageLocally(id)

    suspend fun react(
        reaction: Reaction,
        options: SendOptions? = null,
    ): MessageId = client().raw.conversations().reactToMessage(id, reaction, options)

    suspend fun reply(
        text: String,
        options: SendOptions? = null,
    ): MessageId = client().raw.conversations().replyToMessage(id, encodeText(text), options)

    suspend fun reply(
        content: EncodedContent,
        options: SendOptions? = null,
    ): MessageId = client().raw.conversations().replyToMessage(id, content, options)

    suspend fun reply(
        codec: SDKContentCodec,
        value: Any,
        options: SendOptions? = null,
    ): MessageId = reply(codec.encode(value), options)

    suspend fun parent(): Message? = inReplyTo?.id?.let { client().raw.conversations().getMessageById(it) }

    suspend fun conversation(): Conversation? = client().raw.conversations().getById(conversationId)

    fun client(): SDKClient =
        ClientRegistry.get(data.clientKey)
            ?: throw clientClosedError()

    override fun equals(other: Any?): Boolean =
        other is Message &&
            id == other.id && data.clientKey == other.data.clientKey &&
            data.conversationId == other.data.conversationId && data.topic == other.data.topic &&
            data.deliveryCursor == other.data.deliveryCursor &&
            data.senderInboxId == other.data.senderInboxId && data.sentAt == other.data.sentAt &&
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
        result = 31 * result + (data.deliveryCursor?.hashCode() ?: 0)
        result = 31 * result + data.conversationId.hashCode()
        result = 31 * result + data.senderInboxId.hashCode()
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
            id == other.id && senderInboxId == other.senderInboxId && sentAt == other.sentAt &&
                kind == other.kind && deliveryStatus == other.deliveryStatus &&
                contentType == other.contentType && fallback == other.fallback &&
                content.deepEquals(other.content) &&
                encoded.deepEquals(other.encoded)
        }
    }

private fun ReplyParent?.deepHashCode(): Int {
    if (this == null) return 0
    var result = id.hashCode()
    result = 31 * result + senderInboxId.hashCode()
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
