import XmtpSdk

func invalidCodecRecord(_: Group, _ message: Message) async throws {
    _ = try await message.reply(DeleteMessageCodec(), value: StandardContent.text("wrong"))
}
