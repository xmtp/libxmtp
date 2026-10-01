import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    message.reply(codec = ReactionV2Codec(), value = StandardContent.Text("wrong"))
}
