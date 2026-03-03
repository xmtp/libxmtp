import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class EditMessageCodecTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	func testCanEncodeAndDecodeEditMessage() async throws {
		let codec = EditMessageCodec()

		let original = EditMessageRequest(messageId: "message-to-edit-123")

		let encoded = try codec.encode(content: original)
		let decoded = try codec.decode(content: encoded)

		XCTAssertEqual(original.messageId, decoded.messageId)
	}

	func testEditMessageCodecFallback() throws {
		let codec = EditMessageCodec()
		let content = EditMessageRequest(messageId: "any-id")
		let fallback = try codec.fallback(content: content)
		XCTAssertNil(fallback)
	}

	func testEditMessageCodecShouldPush() throws {
		let codec = EditMessageCodec()
		let content = EditMessageRequest(messageId: "any-id")
		let shouldPush = try codec.shouldPush(content: content)
		XCTAssertFalse(shouldPush)
	}

	func testEditMessageCodecContentType() {
		let codec = EditMessageCodec()
		XCTAssertEqual(codec.contentType, ContentTypeEditMessageRequest)
		XCTAssertEqual(codec.contentType.authorityID, "xmtp.org")
		XCTAssertEqual(codec.contentType.typeID, "editMessage")
		XCTAssertEqual(codec.contentType.versionMajor, 1)
		XCTAssertEqual(codec.contentType.versionMinor, 0)
	}

	func testContentTypeEditMessageRequestValues() {
		XCTAssertEqual(ContentTypeEditMessageRequest.authorityID, "xmtp.org")
		XCTAssertEqual(ContentTypeEditMessageRequest.typeID, "editMessage")
		XCTAssertEqual(ContentTypeEditMessageRequest.versionMajor, 1)
		XCTAssertEqual(ContentTypeEditMessageRequest.versionMinor, 0)
	}

	func testCanSendAndReceiveEditMessage() async throws {
		Client.register(codec: EditMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let editRequest = EditMessageRequest(messageId: "test-message-id-123")

		try await alixConversation.send(
			content: editRequest,
			options: .init(contentType: ContentTypeEditMessageRequest)
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: EditMessageRequest? = try messages.first?.content()
			XCTAssertNotNil(content)
			XCTAssertEqual(content?.messageId, "test-message-id-123")
		}

		try fixtures.cleanUpDatabases()
	}

	func testEditMessageRequestEquatable() {
		let request1 = EditMessageRequest(messageId: "id-1")
		let request2 = EditMessageRequest(messageId: "id-1")
		let request3 = EditMessageRequest(messageId: "id-2")

		XCTAssertEqual(request1, request2)
		XCTAssertNotEqual(request1, request3)
	}

	func testEditMessageRequestCodable() throws {
		let original = EditMessageRequest(messageId: "test-id")

		let encoder = JSONEncoder()
		let data = try encoder.encode(original)

		let decoder = JSONDecoder()
		let decoded = try decoder.decode(EditMessageRequest.self, from: data)

		XCTAssertEqual(original, decoded)
	}

	func testReceiverCanDecodeEditMessageFromListMessages() async throws {
		Client.register(codec: EditMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let alixGroup = try await alixClient.conversations.newGroup(with: [boClient.inboxID])

		try await boClient.conversations.sync()
		let boGroup = try boClient.conversations.findGroup(groupId: alixGroup.id)
		XCTAssertNotNil(boGroup)

		let editRequest = EditMessageRequest(messageId: "message-id-to-edit-456")

		try await alixGroup.send(
			content: editRequest,
			options: .init(contentType: ContentTypeEditMessageRequest)
		)

		try await alixGroup.sync()
		try await boGroup?.sync()

		let boMessages = try await boGroup?.messages()
		let editMessage = boMessages?.first {
			(try? $0.encodedContent.type.typeID) == "editMessage"
		}

		XCTAssertNotNil(editMessage)
		let content: EditMessageRequest? = try editMessage?.content()
		XCTAssertNotNil(content)
		XCTAssertEqual(content?.messageId, "message-id-to-edit-456")

		try fixtures.cleanUpDatabases()
	}

	func testEditMessageContentTypeInListMessages() async throws {
		Client.register(codec: EditMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let editRequest = EditMessageRequest(messageId: "test-msg-789")

		try await alixConversation.send(
			content: editRequest,
			options: .init(contentType: ContentTypeEditMessageRequest)
		)

		let messages = try await alixConversation.messages()
		let editMsg = messages.first {
			(try? $0.encodedContent.type.typeID) == "editMessage"
		}

		XCTAssertNotNil(editMsg)
		XCTAssertEqual(try editMsg?.encodedContent.type.authorityID, "xmtp.org")
		XCTAssertEqual(try editMsg?.encodedContent.type.typeID, "editMessage")

		let decoded: EditMessageRequest? = try editMsg?.content()
		XCTAssertNotNil(decoded)
		XCTAssertEqual(decoded?.messageId, "test-msg-789")

		try fixtures.cleanUpDatabases()
	}
}
