//
//  ReplyTests.swift
//
//
//  Created by Naomi Plasterer on 7/26/23.
//
import Foundation

import XCTest
@testable import XMTP

@available(iOS 15, *)
class ReplyTests: XCTestCase {
	func testCanUseReplyCodec() async throws {
		Client.register(codec: ReplyCodec())

		let fixtures = await fixtures()
		let conversation = try await fixtures.aliceClient.conversations.newConversation(with: fixtures.bobClient.address)

		try await conversation.send(text: "hey alice 2 bob")

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

		let updatedMessages = try await conversation.messages()

		let message = try await conversation.messages()[0]
		let content: Reply = try message.content()
		XCTAssertEqual("Hello", content.content as? String)
		XCTAssertEqual(messageToReply.id, content.reference)
		XCTAssertEqual(ContentTypeText, content.contentType)
	}
}
