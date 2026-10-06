import Foundation
import XmtpSdk

/// Each call passes a value of the wrong type to a typed codec, or uses
/// the removed untyped codec protocol. None may compile.
func consumeCodecTypes(_ group: Group, _ dm: Dm, _ message: Message) async throws {
	_ = try TextCodec().encode(1)
	_ = try await group.send(TextCodec(), value: 1)
	_ = try await group.prepareMessage(TextCodec(), value: 1)
	_ = try await dm.send(TextCodec(), value: 1)
	_ = try await dm.prepareMessage(TextCodec(), value: 1)
	_ = try await Conversation.dm(dm: dm).send(TextCodec(), value: 1)
	_ = try await message.reply(TextCodec(), value: 1)
	let _: (any SDKContentCodec)? = nil
}
