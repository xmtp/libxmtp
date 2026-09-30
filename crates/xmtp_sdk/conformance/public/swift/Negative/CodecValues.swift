import XmtpSdk

/// A typed codec takes only its own value type (P9).
func consumeWrongCodecValues(_ group: Group, _ message: Message) async throws {
    _ = try TextCodec().encode(1)
    _ = try await group.send(TextCodec(), value: 1)
    _ = try await message.reply(TextCodec(), value: 1)
}
