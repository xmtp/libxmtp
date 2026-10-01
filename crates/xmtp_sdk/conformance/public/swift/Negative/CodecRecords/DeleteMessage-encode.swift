import XmtpSdk

func invalidCodecRecord(_: Group, _: Message) async throws {
    _ = try DeleteMessageCodec().encode(StandardContent.text("wrong"))
}
