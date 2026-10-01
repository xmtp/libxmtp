import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    ReplyCodec().encode(StandardContent.Text("wrong"))
}
