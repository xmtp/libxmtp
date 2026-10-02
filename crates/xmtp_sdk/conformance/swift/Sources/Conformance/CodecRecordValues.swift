import Foundation
import XmtpSdk

private func matchingRecordWire<C: ContentCodec>(
    _ codec: C, _ value: C.Value, _ expected: EncodedContent,
    equal: (C.Value, C.Value) -> Bool
) throws -> Bool {
    let encoded = try codec.encode(value)
    let decoded = try codec.decode(encoded)
    return try sameEncoded(encoded, expected) && equal(decoded, value) && sameEncoded(codec.encode(decoded), expected) &&
        codec.fallback(value) == expected.fallback &&
        codec.shouldPush(value) == catalogueContentTypeShouldPush(contentType: expected.type)
}

func matchesRust(_ codec: ReactionV2Codec, _ value: ReactionV2Content, _ expected: EncodedContent) throws -> Bool {
    try matchingRecordWire(codec, value, expected) {
        $0.reference == $1.reference && $0.referenceInboxId == $1.referenceInboxId && $0.reaction == $1.reaction
    }
}

func matchesRust(_ codec: ReplyCodec, _ value: ReplyContent, _ expected: EncodedContent) throws -> Bool {
    try matchingRecordWire(codec, value, expected) {
        $0.reference == $1.reference && $0.referenceInboxId == $1.referenceInboxId && sameEncoded($0.content, $1.content)
    }
}

func matchesRust(_ codec: DeleteMessageCodec, _ value: DeleteMessageContent, _ expected: EncodedContent) throws -> Bool {
    try matchingRecordWire(codec, value, expected) { $0.messageId == $1.messageId }
}

// verifies: CTYPE-007, CTYPE-026
func checkCodecRecordValues() throws {
    let reference: MessageId = String(repeating: "d", count: 64)
    let inboxes: [InboxId?] = [nil, String(repeating: "b", count: 64)]
    let reaction = Reaction(content: "👍", action: .added, schema: .unicode)
    var nested = try TextCodec().encode("nested bytes")
    nested.parameters = ["key": "value"]
    nested.fallback = "nested fallback"
    guard ReactionV2Content(reference: reference, reaction: reaction).referenceInboxId == nil,
          ReplyContent(reference: reference, content: nested).referenceInboxId == nil
    else { throw ConformanceFailure("codec inbox default is not absent") }
    for inbox in inboxes {
        let reactionValue = ReactionV2Content(reference: reference, referenceInboxId: inbox, reaction: reaction)
        let reactionWire = try encodeStandard(value: .reaction(reference: reference, referenceInboxId: inbox, reaction: reaction))
        guard try matchesRust(ReactionV2Codec(), reactionValue, reactionWire) else {
            throw ConformanceFailure("ReactionV2Content lost a field or changed wire bytes")
        }
        let replyValue = ReplyContent(reference: reference, referenceInboxId: inbox, content: nested)
        let replyWire = try encodeStandard(value: .reply(reference: reference, referenceInboxId: inbox, content: nested))
        guard try matchesRust(ReplyCodec(), replyValue, replyWire) else {
            throw ConformanceFailure("ReplyContent lost a field or changed wire bytes")
        }
    }
    let deleteValue = DeleteMessageContent(messageId: reference)
    let deleteWire = try encodeStandard(value: .deleteMessage(messageId: reference))
    guard try matchesRust(DeleteMessageCodec(), deleteValue, deleteWire) else {
        throw ConformanceFailure("DeleteMessageContent lost its message id or changed wire bytes")
    }
    print("Swift codec records preserve every field, optional inbox, and Rust wire bytes")
}
