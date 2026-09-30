import uniffi.xmtp_sdk.*

// Each call passes a value of the wrong type to a typed codec (P9), or uses
// the removed untyped codec interface. None may compile.
// Known gap, waiting for an owner decision: ReactionV2Codec, ReplyCodec, and
// DeleteMessageCodec take the whole StandardContent, so another variant
// compiles. codecPolicyFailureNeverPublishes checks it at run time.
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
