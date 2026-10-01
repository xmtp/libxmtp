import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    ReactionV2Codec().encode(StandardContent.Text("wrong"))
}
