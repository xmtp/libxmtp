import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    group.send(codec = DeleteMessageCodec(), value = StandardContent.Text("wrong"))
}
