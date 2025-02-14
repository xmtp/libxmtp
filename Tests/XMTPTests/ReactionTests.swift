import Foundation
import XCTest
import LibXMTP

@testable import XMTPiOS

@available(iOS 15, *)
class ReactionTests: XCTestCase {
    
    func testCanDecodeLegacyForm() async throws {
        let codec = ReactionCodec()
        
        // This is how clients send reactions now.
        let canonicalEncoded = EncodedContent.with {
            $0.type = ContentTypeReaction
            $0.content = Data(
    """
    {
      "action": "added",
      "content": "smile",
      "reference": "abc123",
      "schema": "shortcode"
    }
    """.utf8)
        }
        
        // Previously, some clients sent reactions like this.
        // So we test here to make sure we can still decode them.
        let legacyEncoded = EncodedContent.with {
            $0.type = ContentTypeReaction
            $0.parameters = [
                "action": "added",
                "reference": "abc123",
                "schema": "shortcode",
            ]
            $0.content = Data("smile".utf8)
        }
        
        let fixtures = try await fixtures()
        let canonical = try codec.decode(
            content: canonicalEncoded)
        let legacy = try codec.decode(
            content: legacyEncoded)
        
        XCTAssertEqual(ReactionAction.added, canonical.action)
        XCTAssertEqual(ReactionAction.added, legacy.action)
        XCTAssertEqual("smile", canonical.content)
        XCTAssertEqual("smile", legacy.content)
        XCTAssertEqual("abc123", canonical.reference)
        XCTAssertEqual("abc123", legacy.reference)
        XCTAssertEqual(ReactionSchema.shortcode, canonical.schema)
        XCTAssertEqual(ReactionSchema.shortcode, legacy.schema)
    }
    
    func testCanUseReactionCodec() async throws {
        let fixtures = try await fixtures()
        let conversation = try await fixtures.alixClient.conversations
            .newConversation(with: fixtures.boClient.address)
        
        Client.register(codec: ReactionCodec())
        
        _ = try await conversation.send(text: "hey alix 2 bo")
        
        let messageToReact = try await conversation.messages()[0]
        
        let reaction = Reaction(
            reference: messageToReact.id,
            action: .added,
            content: "U+1F603",
            schema: .unicode
        )
        
        try await conversation.send(
            content: reaction,
            options: .init(contentType: ContentTypeReaction)
        )
        
        _ = try await conversation.messages()
        
        let message = try await conversation.messages()[0]
        let content: Reaction = try message.content()
        XCTAssertEqual("U+1F603", content.content)
        XCTAssertEqual(messageToReact.id, content.reference)
        XCTAssertEqual(ReactionAction.added, content.action)
        XCTAssertEqual(ReactionSchema.unicode, content.schema)
    }
    
    func testCanDecodeEmptyForm() async throws {
        let codec = ReactionCodec()
        
        // This is how clients send reactions now.
        let canonicalEncoded = EncodedContent.with {
            $0.type = ContentTypeReaction
            $0.content = Data(
    """
    {
      "action": "",
      "content": "smile",
      "reference": "",
      "schema": ""
    }
    """.utf8)
        }
        
        // Previously, some clients sent reactions like this.
        // So we test here to make sure we can still decode them.
        let legacyEncoded = EncodedContent.with {
            $0.type = ContentTypeReaction
            $0.parameters = [
                "action": "",
                "reference": "",
                "schema": "",
            ]
            $0.content = Data("smile".utf8)
        }
        
        let fixtures = try await fixtures()
        
        let canonical = try codec.decode(
            content: canonicalEncoded)
        let legacy = try codec.decode(
            content: legacyEncoded)
        
        XCTAssertEqual(ReactionAction.unknown, canonical.action)
        XCTAssertEqual(ReactionAction.unknown, legacy.action)
        XCTAssertEqual("smile", canonical.content)
        XCTAssertEqual("smile", legacy.content)
        XCTAssertEqual("", canonical.reference)
        XCTAssertEqual("", legacy.reference)
        XCTAssertEqual(ReactionSchema.unknown, canonical.schema)
        XCTAssertEqual(ReactionSchema.unknown, legacy.schema)
    }
        
    func testCanUseReactionV2Codec() async throws {
        Client.register(codec: ReactionV2Codec())
        
        let fixtures = try await fixtures()
        let conversation = try await fixtures.alixClient.conversations
            .newConversation(with: fixtures.boClient.address)
        
        _ = try await conversation.send(text: "hey alice 2 bob")
        
        let messageToReact = try await conversation.messages()[0]
        
        let reaction = FfiReaction(
            reference: messageToReact.id,
            referenceInboxId: "",
            action: .added,
            content: "U+1F603",
            schema: .unicode
        )
        
        try await conversation.send(
            content: reaction,
            options: .init(contentType: ContentTypeReactionV2)
        )
        
        let messages = try await conversation.messages()
        XCTAssertEqual(messages.count, 3)
        
        let content: FfiReaction = try messages[0].content()
        XCTAssertEqual("U+1F603", content.content)
        XCTAssertEqual(messageToReact.id, content.reference)
        XCTAssertEqual(FfiReactionAction.added, content.action)
        XCTAssertEqual(FfiReactionSchema.unicode, content.schema)
        
        let messagesWithReactions = try await conversation.messagesWithReactions()
        XCTAssertEqual(messagesWithReactions.count, 1)
        XCTAssertEqual(messagesWithReactions[0].id, messageToReact.id)
        
        let reactionContent: FfiReaction = try messagesWithReactions[0].childMessages![0].content()
        XCTAssertEqual(reactionContent.reference, messageToReact.id)
    }
    
    func testCanMixReactionTypes() async throws {
        // Register both codecs
        Client.register(codec: ReactionV2Codec())
        Client.register(codec: ReactionCodec())
        
        let fixtures = try await fixtures()
        let conversation = try await fixtures.alixClient.conversations
            .newConversation(with: fixtures.boClient.address)
        
        // Send initial message
        _ = try await conversation.send(text: "hey alice 2 bob")
        let messageToReact = try await conversation.messages()[0]
        
        // Send V2 reaction
        let reactionV2 = FfiReaction(
            reference: messageToReact.id,
            referenceInboxId: fixtures.alixClient.inboxID,
            action: .added,
            content: "U+1F603",
            schema: FfiReactionSchema.unicode
        )
        try await conversation.send(
            content: reactionV2,
            options: .init(contentType: ContentTypeReactionV2)
        )
        
        // Send V1 reaction
        let reactionV1 = Reaction(
            reference: messageToReact.id,
            action: .added,
            content: "U+1F604", // Different emoji to distinguish
            schema: .unicode
        )
        try await conversation.send(
            content: reactionV1,
            options: .init(contentType: ContentTypeReaction)
        )
        
        // Verify both reactions appear in messagesWithReactions
        let messagesWithReactions = try await conversation.messagesWithReactions()
        
        XCTAssertEqual(1, messagesWithReactions.count)
        XCTAssertEqual(messageToReact.id, messagesWithReactions[0].id)
        XCTAssertEqual(2, messagesWithReactions[0].childMessages?.count)
        
        
        // Verify both reaction contents
        let childContent1: Reaction = try messagesWithReactions[0].childMessages![0].content()
        XCTAssertEqual("U+1F604", childContent1.content)
        
        let childContent2: FfiReaction = try messagesWithReactions[0].childMessages![1].content()
        XCTAssertEqual("U+1F603", childContent2.content)
        XCTAssertEqual(.unicode, childContent2.schema)
    }
}
