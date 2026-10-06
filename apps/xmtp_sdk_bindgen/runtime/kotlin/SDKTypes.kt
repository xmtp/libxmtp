package uniffi.xmtp_sdk

import java.lang.ref.WeakReference
import java.time.Instant
import java.util.concurrent.ConcurrentHashMap

/**
 * A content codec with a typed value (Ref Public surface, Host codecs). The
 * send helpers run its steps before the send: `encode`, then `fallback` when
 * the envelope has none, then `shouldPush` when the send has no explicit
 * `shouldPush` option and the type is not a catalogue type. A step that
 * throws fails the send with `XmtpException.CodecEncodeFailed`, and the SDK
 * makes no publish attempt.
 */
interface ContentCodec<T : Any> {
    val type: ContentTypeId

    fun encode(value: T): EncodedContent

    fun decode(encoded: EncodedContent): T

    /** Text for recipients without this codec. Default: no fallback. */
    fun fallback(value: T): String? = null

    /** Whether sending this value notifies recipients. Default: it does. */
    fun shouldPush(value: T): Boolean = true
}

/** The registry key of a content type: authority, type ID, and major version. */
internal data class ContentCodecKey(
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
        val rawBytes: ByteArray,
        val value: Any?,
        val error: ErrorDetails?,
    ) : SDKMessageContent()

    data class Unknown(
        val encoded: EncodedContent?,
        val rawBytes: ByteArray,
        val error: ErrorDetails,
    ) : SDKMessageContent()
}

sealed class SDKReplyContent {
    data class Standard(
        val value: MessageBody,
    ) : SDKReplyContent()

    data class Custom(
        val encoded: EncodedContent,
        val rawBytes: ByteArray,
        val value: Any?,
        val error: ErrorDetails?,
    ) : SDKReplyContent()

    data class Unknown(
        val encoded: EncodedContent?,
        val rawBytes: ByteArray,
        val error: ErrorDetails,
    ) : SDKReplyContent()
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
            when (val decoded = ClientRegistry.get(clientKey)?.decodeCustom(body.encoded, body.rawBytes)) {
                is SDKMessageContent.Custom -> {
                    SDKReplyContent.Custom(
                        body.encoded,
                        body.rawBytes,
                        decoded.value,
                        decoded.error,
                    )
                }

                is SDKMessageContent.Unknown -> {
                    SDKReplyContent.Unknown(
                        decoded.encoded,
                        decoded.rawBytes,
                        decoded.error,
                    )
                }

                else -> {
                    SDKReplyContent.Custom(body.encoded, body.rawBytes, null, closedContentDetails())
                }
            }
        }

        is MessageBody.Unknown -> {
            SDKReplyContent.Unknown(body.encoded, body.rawBytes, body.error)
        }

        else -> {
            SDKReplyContent.Standard(body)
        }
    }

/** A received or stored message. [MessageFields] holds its fields and equality. */
class Message(
    data: MessageData,
) : MessageFields(data) {
    val inReplyToContent: SDKReplyContent? =
        data.inReplyTo?.let { decodeReplyBody(it.content, data.clientKey) }
    val replyContent: SDKReplyContent? =
        (data.content as? MessageContent.Reply)?.let { decodeReplyBody(it.body, data.clientKey) }
    val content: SDKMessageContent =
        if (replyContent is SDKReplyContent.Custom && replyContent.error?.code == "CodecDecodeFailed") {
            SDKMessageContent.Unknown(data.encoded, data.rawBytes, checkNotNull(replyContent.error))
        } else {
            when (val body = data.content) {
                is MessageContent.Custom -> {
                    ClientRegistry.get(data.clientKey)?.decodeCustom(body.encoded, body.rawBytes)
                        ?: SDKMessageContent.Custom(body.encoded, body.rawBytes, null, closedContentDetails())
                }

                is MessageContent.Unknown -> {
                    SDKMessageContent.Unknown(body.encoded, body.rawBytes, body.error)
                }

                else -> {
                    SDKMessageContent.Standard(body)
                }
            }
        }

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

    /**
     * Reply with a value of a typed codec. The codec's fallback applies to the
     * nested envelope; the reply keeps the reply type's push default unless
     * `options.shouldPush` is set. A failed codec step is `CodecEncodeFailed`,
     * with no publish attempt.
     */
    suspend fun <T : Any> reply(
        codec: ContentCodec<T>,
        value: T,
        options: SendOptions? = null,
    ): MessageId = reply(replyEnvelope(codec, value), options)

    suspend fun parent(): Message? = inReplyTo?.id?.let { client().raw.conversations().getMessageById(it) }

    suspend fun conversation(): Conversation? = client().raw.conversations().getById(conversationId)

    fun client(): SDKClient =
        ClientRegistry.get(data.clientKey)
            ?: throw clientClosedError()
}

private fun closedContentDetails() = ErrorDetails("ClientClosed", ErrorCategory.LIFECYCLE, false, "client is closed")

private fun clientClosedError() =
    XmtpException.ClientClosed(
        ErrorDetails("ClientClosed", ErrorCategory.LIFECYCLE, false, "client is closed"),
    )

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
