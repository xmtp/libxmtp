import XmtpSdk

func invalidCodecRecord(_: Group, _: Message) async throws {
    _ = try ReplyCodec().encode(StandardContent.text("wrong"))
}
