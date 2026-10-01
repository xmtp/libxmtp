import XmtpSdk

func invalidCodecRecord(_: Group, _: Message) async throws {
    _ = try ReactionV2Codec().encode(StandardContent.text("wrong"))
}
