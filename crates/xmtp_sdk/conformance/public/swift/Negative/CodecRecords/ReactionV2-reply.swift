import XmtpSdk

func invalidCodecRecord(_: Group, _ message: Message) async throws {
    _ = try await message.reply(ReactionV2Codec(), value: StandardContent.text("wrong"))
}
