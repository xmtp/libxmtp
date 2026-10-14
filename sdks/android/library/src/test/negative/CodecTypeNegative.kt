import uniffi.xmtp_sdk.*

// Each call passes a value of the wrong type to a typed codec, or uses
// the removed untyped codec interface. None may compile.
suspend fun consumeCodecTypes(
    group: Group,
    dm: Dm,
    conversation: Conversation,
    message: Message,
) {
    TextCodec().encode(1)
    group.send(TextCodec(), 1)
    group.prepareMessage(TextCodec(), 1)
    dm.send(TextCodec(), 1)
    dm.prepareMessage(TextCodec(), 1)
    conversation.send(TextCodec(), 1)
    message.reply(TextCodec(), 1)
    val removed: SDKContentCodec? = null
}
