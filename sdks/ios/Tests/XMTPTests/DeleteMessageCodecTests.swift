import Foundation
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class DeleteMessageCodecTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	// MARK: - Codec Tests

	func testCanEncodeAndDecodeDeleteMessage() async throws {
		let codec = DeleteMessageCodec()

		let original = DeleteMessageRequest(messageId: "message-to-delete-123")

		let encoded = try codec.encode(content: original)
		let decoded = try codec.decode(content: encoded)

		XCTAssertEqual(original, decoded)
		XCTAssertEqual(original.messageId, decoded.messageId)
	}

	func testDeleteMessageCodecFallback() throws {
		let codec = DeleteMessageCodec()
		let content = DeleteMessageRequest(messageId: "any-id")
		let fallback = try codec.fallback(content: content)
		XCTAssertNil(fallback)
	}

	func testDeleteMessageCodecShouldPush() throws {
		let codec = DeleteMessageCodec()
		let content = DeleteMessageRequest(messageId: "any-id")
		let shouldPush = try codec.shouldPush(content: content)
		XCTAssertFalse(shouldPush)
	}

	func testDeleteMessageCodecContentType() {
		let codec = DeleteMessageCodec()
		XCTAssertEqual(codec.contentType, ContentTypeDeleteMessageRequest)
		XCTAssertEqual(codec.contentType.authorityID, "xmtp.org")
		XCTAssertEqual(codec.contentType.typeID, "deleteMessage")
		XCTAssertEqual(codec.contentType.versionMajor, 1)
		XCTAssertEqual(codec.contentType.versionMinor, 0)
	}

	func testContentTypeDeleteMessageRequestValues() {
		XCTAssertEqual(ContentTypeDeleteMessageRequest.authorityID, "xmtp.org")
		XCTAssertEqual(ContentTypeDeleteMessageRequest.typeID, "deleteMessage")
		XCTAssertEqual(ContentTypeDeleteMessageRequest.versionMajor, 1)
		XCTAssertEqual(ContentTypeDeleteMessageRequest.versionMinor, 0)
	}

	// MARK: - Integration Tests

	func testCanSendAndReceiveDeleteMessage() async throws {
		Client.register(codec: DeleteMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let deleteRequest = DeleteMessageRequest(messageId: "test-message-id-123")

		try await alixConversation.send(
			content: deleteRequest,
			options: .init(contentType: ContentTypeDeleteMessageRequest)
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: DeleteMessageRequest? = try messages.first?.content()
			XCTAssertNotNil(content)
			XCTAssertEqual(content?.messageId, "test-message-id-123")
		}

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Unit Tests

	func testDeleteMessageRequestEquatable() {
		let request1 = DeleteMessageRequest(messageId: "id-1")
		let request2 = DeleteMessageRequest(messageId: "id-1")
		let request3 = DeleteMessageRequest(messageId: "id-2")

		XCTAssertEqual(request1, request2)
		XCTAssertNotEqual(request1, request3)
	}

	func testDeleteMessageRequestCodable() throws {
		let original = DeleteMessageRequest(messageId: "test-id")

		let encoder = JSONEncoder()
		let data = try encoder.encode(original)

		let decoder = JSONDecoder()
		let decoded = try decoder.decode(DeleteMessageRequest.self, from: data)

		XCTAssertEqual(original, decoded)
	}

	// MARK: - List Messages Tests (not enriched)

	func testReceiverCanDecodeDeleteMessageFromListMessages() async throws {
		Client.register(codec: DeleteMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let alixGroup = try await alixClient.conversations.newGroup(with: [boClient.inboxID])

		try await boClient.conversations.sync()
		let boGroup = try boClient.conversations.findGroup(groupId: alixGroup.id)
		XCTAssertNotNil(boGroup)

		let deleteRequest = DeleteMessageRequest(messageId: "message-id-to-delete-456")

		try await alixGroup.send(
			content: deleteRequest,
			options: .init(contentType: ContentTypeDeleteMessageRequest)
		)

		try await alixGroup.sync()
		try await boGroup?.sync()

		// Receiver reads using messages() - not enrichedMessages()
		let boMessages = try await boGroup?.messages()
		let deleteMessage = boMessages?.first {
			(try? $0.encodedContent.type.typeID) == "deleteMessage"
		}

		XCTAssertNotNil(deleteMessage)
		let content: DeleteMessageRequest? = try deleteMessage?.content()
		XCTAssertNotNil(content)
		XCTAssertEqual(content?.messageId, "message-id-to-delete-456")

		try fixtures.cleanUpDatabases()
	}

	func testDeleteMessageContentTypeInListMessages() async throws {
		Client.register(codec: DeleteMessageCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let deleteRequest = DeleteMessageRequest(messageId: "test-msg-789")

		try await alixConversation.send(
			content: deleteRequest,
			options: .init(contentType: ContentTypeDeleteMessageRequest)
		)

		// Using messages() API to verify content type is preserved
		let messages = try await alixConversation.messages()
		let deleteMsg = messages.first {
			(try? $0.encodedContent.type.typeID) == "deleteMessage"
		}

		XCTAssertNotNil(deleteMsg)
		XCTAssertEqual(try deleteMsg?.encodedContent.type.authorityID, "xmtp.org")
		XCTAssertEqual(try deleteMsg?.encodedContent.type.typeID, "deleteMessage")

		// Verify we can decode the content
		let decoded: DeleteMessageRequest? = try deleteMsg?.content()
		XCTAssertNotNil(decoded)
		XCTAssertEqual(decoded?.messageId, "test-msg-789")

		try fixtures.cleanUpDatabases()
	}
}
