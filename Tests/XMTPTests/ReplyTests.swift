import Foundation
import XCTest

@testable import XMTPiOS

@available(iOS 15, *)
class ReplyTests: XCTestCase {
	func testCanUseReplyCodec() async throws {
		let fixtures = try await fixtures()
		let conversation = try await fixtures.alixClient.conversations
			.newConversation(with: fixtures.boClient.address)

		Client.register(codec: ReplyCodec())

		_ = try await conversation.send(text: "hey alix 2 bo")

		let messageToReply = try await conversation.messages()[0]

		let reply = Reply(
			reference: messageToReply.id,
			content: "Hello",
			contentType: ContentTypeText
		)

		try await conversation.send(
			content: reply,
			options: .init(contentType: ContentTypeReply)
		)

		_ = try await conversation.messages()

		let message = try await conversation.messages()[0]
		let content: Reply = try message.content()
		XCTAssertEqual("Hello", content.content as? String)
		XCTAssertEqual(messageToReply.id, content.reference)
		XCTAssertEqual(ContentTypeText, content.contentType)
	}
}
