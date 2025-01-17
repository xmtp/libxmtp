import Foundation
import XCTest

@testable import XMTPiOS

@available(iOS 15, *)
class ReadReceiptTests: XCTestCase {
	func testCanUseReadReceiptCodec() async throws {
		let fixtures = try await fixtures()
		Client.register(codec: ReadReceiptCodec())

		let conversation = try await fixtures.alixClient.conversations
			.newConversation(with: fixtures.boClient.address)

		_ = try await conversation.send(text: "hey alix 2 bo")

		let read = ReadReceipt()

		try await conversation.send(
			content: read,
			options: .init(contentType: ContentTypeReadReceipt)
		)

		_ = try await conversation.messages()

		let message = try await conversation.messages()[0]
		let contentType: String = try message.encodedContent.type.typeID
		XCTAssertEqual("readReceipt", contentType)

		let convos = try await fixtures.alixClient.conversations.list()
		let contentType2: String = try await convos.first!.lastMessage()!
			.encodedContent.type.typeID
		XCTAssertEqual("text", contentType2)

	}
}
