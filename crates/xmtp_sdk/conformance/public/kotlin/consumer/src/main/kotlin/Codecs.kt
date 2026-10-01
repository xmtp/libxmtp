import uniffi.xmtp_sdk.*

/** A typed app value. */
data class Point(
    val x: Int,
    val y: Int,
)

/** An app codec with a typed value and both send hooks. */
class PointCodec : ContentCodec<Point> {
    override val type = ContentTypeId("example.org", "point", 1u, 0u)

    override fun encode(value: Point) = EncodedContent(type, emptyMap(), null, "${value.x},${value.y}".toByteArray())

    override fun decode(encoded: EncodedContent): Point {
        val parts = encoded.content.decodeToString().split(",")
        return Point(parts.first().toIntOrNull() ?: 0, parts.last().toIntOrNull() ?: 0)
    }

    override fun fallback(value: Point) = "point ${value.x},${value.y}"

    override fun shouldPush(value: Point) = value.x != 0
}

// verifies: CTYPE-017

/** Typed codec sends, replies, and mixed registration on the installed library. */
suspend fun consumeTypedCodecs(
    signer: Signer,
    options: ClientOptions,
    group: Group,
    dm: Dm,
    message: Message,
) {
    val point = Point(1, 2)
    val sent: MessageId = group.send(PointCodec(), point)
    val text: MessageId = dm.send(TextCodec(), "text", SendOptions(shouldPush = false))
    val prepared: MessageId = group.prepareMessage(PointCodec(), point)
    val markdown: MessageId = Conversation.Group(group).send(MarkdownCodec(), "**md**")
    val reply: MessageId = message.reply(PointCodec(), point)
    val encoded: EncodedContent = PointCodec().encode(point)
    // Codecs of different value types register together.
    val client = SDKClient.create(signer, options, codecs = listOf(PointCodec(), TextCodec()))
    client.end()
    println("$sent $text $prepared $markdown $reply $encoded")
}

fun receivedDetails(message: Message): String? {
    val encoded: EncodedContent? = message.encoded
    val type: ContentTypeId? = message.contentType
    check(message.rawBytes.isNotEmpty() || encoded == null || type != null)
    return when (val content = message.content) {
        is SDKMessageContent.Unknown -> content.error.code
        is SDKMessageContent.Custom -> content.error?.code
        is SDKMessageContent.Standard -> null
    }
}
