import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    group.send(codec = ReplyCodec(), value = StandardContent.Text("wrong"))
}
