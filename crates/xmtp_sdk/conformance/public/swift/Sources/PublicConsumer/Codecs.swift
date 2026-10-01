import Foundation
import XmtpSdk

/// An app codec with a typed value and both send hooks.
public struct PointCodec: ContentCodec {
    public struct Point: Sendable {
        public let x: Int
        public let y: Int
    }

    public let type = ContentTypeId(authorityId: "example.org", typeId: "point", versionMajor: 1, versionMinor: 0)

    public func encode(_ value: Point) throws -> EncodedContent {
        EncodedContent(type: type, content: Data("\(value.x),\(value.y)".utf8))
    }

    public func decode(_ encoded: EncodedContent) throws -> Point {
        let parts = String(decoding: encoded.content, as: UTF8.self).split(separator: ",")
        return Point(x: Int(parts.first ?? "") ?? 0, y: Int(parts.last ?? "") ?? 0)
    }

    public func fallback(_ value: Point) throws -> String? {
        "point \(value.x),\(value.y)"
    }

    public func shouldPush(_ value: Point) throws -> Bool {
        value.x != 0
    }
}

// verifies: CTYPE-017
/// Typed codec sends, replies, and mixed registration on the installed product.
public func consumeTypedCodecs(
    _ signer: Signer, _ options: ClientOptions, _ group: Group, _ dm: Dm, _ message: Message
) async throws {
    let point = PointCodec.Point(x: 1, y: 2)
    let _: MessageId = try await group.send(PointCodec(), value: point)
    let _: MessageId = try await dm.send(TextCodec(), value: "text", options: SendOptions(shouldPush: false))
    let _: MessageId = try await group.prepareMessage(PointCodec(), value: point)
    let _: MessageId = try await Conversation.group(group: group).send(MarkdownCodec(), value: "**md**")
    let _: MessageId = try await message.reply(PointCodec(), value: point)
    let _: EncodedContent = try PointCodec().encode(point)
    // Codecs of different value types register together.
    let client = try await SDKClient.create(signer: signer, options: options, codecs: [PointCodec(), TextCodec()])
    try await client.end()
}

func receivedDetails(_ message: Message) -> String? {
    let _: Data = message.rawBytes
    let _: EncodedContent? = message.encoded
    let _: ContentTypeId? = message.contentType
    switch message.content {
    case let .unknown(_, _, error): return error.code
    case let .custom(_, _, _, error): return error?.code
    case .standard: return nil
    }
}

public func consumeCodecRecords(_ group: Group, _ message: Message) async throws {
    let reference: MessageId = String(repeating: "a", count: 64)
    let inbox: InboxId = String(repeating: "b", count: 64)
    let reaction = Reaction(content: "👍", action: .added, schema: .unicode)
    let reactionValue = ReactionV2Content(reference: reference, reaction: reaction)
    _ = ReactionV2Content(reference: reference, referenceInboxId: inbox, reaction: reaction)
    let nested = try TextCodec().encode("reply")
    let replyValue = ReplyContent(reference: reference, content: nested)
    _ = ReplyContent(reference: reference, referenceInboxId: inbox, content: nested)
    let deleteValue = DeleteMessageContent(messageId: reference)
    let _: EncodedContent = try ReactionV2Codec().encode(reactionValue)
    let _: EncodedContent = try ReplyCodec().encode(replyValue)
    let _: EncodedContent = try DeleteMessageCodec().encode(deleteValue)
    let _: MessageId = try await group.send(ReactionV2Codec(), value: reactionValue)
    let _: MessageId = try await group.send(ReplyCodec(), value: replyValue)
    let _: MessageId = try await group.send(DeleteMessageCodec(), value: deleteValue)
    let _: MessageId = try await message.reply(ReactionV2Codec(), value: reactionValue)
    let _: MessageId = try await message.reply(ReplyCodec(), value: replyValue)
    let _: MessageId = try await message.reply(DeleteMessageCodec(), value: deleteValue)
}
