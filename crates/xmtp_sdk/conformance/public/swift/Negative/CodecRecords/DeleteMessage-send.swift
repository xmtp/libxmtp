import XmtpSdk

func invalidCodecRecord(_ group: Group, _: Message) async throws {
    _ = try await group.send(DeleteMessageCodec(), value: StandardContent.text("wrong"))
}
