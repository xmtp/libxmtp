import uniffi.xmtp_sdk.*

// A typed codec takes only its own value type (P9).
suspend fun consumeWrongCodecValues(
    group: Group,
    message: Message,
) {
    TextCodec().encode(1)
    group.send(TextCodec(), 1)
    message.reply(TextCodec(), 1)
}
