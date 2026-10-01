import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    message.reply(codec = ReplyCodec(), value = StandardContent.Text("wrong"))
}
