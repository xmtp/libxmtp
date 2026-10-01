import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    message.reply(codec = DeleteMessageCodec(), value = StandardContent.Text("wrong"))
}
