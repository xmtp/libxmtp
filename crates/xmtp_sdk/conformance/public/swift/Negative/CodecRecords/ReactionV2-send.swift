import XmtpSdk

func invalidCodecRecord(_ group: Group, _: Message) async throws {
    _ = try await group.send(ReactionV2Codec(), value: StandardContent.text("wrong"))
}
