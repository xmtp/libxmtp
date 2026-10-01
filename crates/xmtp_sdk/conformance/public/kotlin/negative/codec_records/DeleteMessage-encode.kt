import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    DeleteMessageCodec().encode(StandardContent.Text("wrong"))
}
