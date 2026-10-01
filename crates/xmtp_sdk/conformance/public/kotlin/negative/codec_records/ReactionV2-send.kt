import uniffi.xmtp_sdk.*

suspend fun invalidCodecRecord(
    group: Group,
    message: Message,
) {
    group.send(codec = ReactionV2Codec(), value = StandardContent.Text("wrong"))
}
