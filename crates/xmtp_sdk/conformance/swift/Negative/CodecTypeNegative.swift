import Foundation
import XmtpSdk

/// Each call passes a value of the wrong type to a typed codec (P9), or uses
/// the removed untyped codec protocol. None may compile.
/// Known gap, waiting for an owner decision: ReactionV2Codec, ReplyCodec, and
/// DeleteMessageCodec take the whole StandardContent, so another variant
/// compiles. codecPolicyFailureNeverPublishes checks it at run time.
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
