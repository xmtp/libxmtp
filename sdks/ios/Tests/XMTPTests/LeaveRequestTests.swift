import Foundation
import XCTest
import XMTPTestHelpers

@testable import XMTPiOS

@available(iOS 16, *)
class LeaveRequestTests: XCTestCase {
	/// Delay for admin worker to process removals (in nanoseconds)
	private static let adminWorkerDelayNs: UInt64 = 3_000_000_000 // 3 seconds

	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	// MARK: - Codec Tests

	func testCanUseLeaveRequestCodec() async throws {
		let codec = LeaveRequestCodec()

		let original = LeaveRequest(authenticatedNote: Data("test note".utf8))
		let encoded = try codec.encode(content: original)
		let decoded = try codec.decode(content: encoded)

		XCTAssertEqual(original, decoded)
		XCTAssertEqual(original.authenticatedNote, decoded.authenticatedNote)
	}

	func testLeaveRequestCodecWithNilNote() async throws {
		let codec = LeaveRequestCodec()

		let original = LeaveRequest(authenticatedNote: nil)
		let encoded = try codec.encode(content: original)
		let decoded = try codec.decode(content: encoded)

		XCTAssertEqual(original, decoded)
		XCTAssertNil(decoded.authenticatedNote)
	}

	func testLeaveRequestCodecFallback() throws {
		let codec = LeaveRequestCodec()
		let content = LeaveRequest()
		let fallback = try codec.fallback(content: content)
		XCTAssertEqual(fallback, "A member has requested leaving the group")
	}

	func testLeaveRequestCodecShouldPush() throws {
		let codec = LeaveRequestCodec()
		let content = LeaveRequest()
		let shouldPush = try codec.shouldPush(content: content)
		XCTAssertFalse(shouldPush)
	}

	func testLeaveRequestCodecContentType() {
		let codec = LeaveRequestCodec()
		XCTAssertEqual(codec.contentType, ContentTypeLeaveRequest)
	}

	func testCanSendAndReceiveLeaveRequestWithCodec() async throws {
		Client.register(codec: LeaveRequestCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let leaveRequest = LeaveRequest(authenticatedNote: Data("random_auth_note".utf8))

		try await alixConversation.send(
			content: leaveRequest,
			options: .init(contentType: ContentTypeLeaveRequest)
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: LeaveRequest? = try messages.first?.content()
			XCTAssertNotNil(content)
			XCTAssertEqual(content?.authenticatedNote, Data("random_auth_note".utf8))
		}

		try fixtures.cleanUpDatabases()
	}

	func testCanSendAndReceiveLeaveRequestWithNilNote() async throws {
		Client.register(codec: LeaveRequestCodec())

		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		let conversation = try await alixClient.conversations.newConversation(with: boClient.inboxID)
		let alixConversation = try XCTUnwrap(conversation)

		let leaveRequest = LeaveRequest(authenticatedNote: nil)

		try await alixConversation.send(
			content: leaveRequest,
			options: .init(contentType: ContentTypeLeaveRequest)
		)

		let messages = try await alixConversation.messages()
		XCTAssertEqual(messages.count, 2)

		if messages.count == 2 {
			let content: LeaveRequest? = try messages.first?.content()
			XCTAssertNotNil(content)
			XCTAssertNil(content?.authenticatedNote)
		}

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Unit Tests for LeaveRequest

	func testLeaveRequestEmptyDataNormalizedToNil() {
		let requestWithEmptyData = LeaveRequest(authenticatedNote: Data())
		XCTAssertNil(requestWithEmptyData.authenticatedNote)

		let requestWithData = LeaveRequest(authenticatedNote: Data("note".utf8))
		XCTAssertNotNil(requestWithData.authenticatedNote)

		let requestWithNil = LeaveRequest(authenticatedNote: nil)
		XCTAssertNil(requestWithNil.authenticatedNote)
	}

	func testLeaveRequestEquatable() {
		let note1 = Data("note".utf8)
		let note2 = Data("note".utf8)
		let note3 = Data("different".utf8)

		let request1 = LeaveRequest(authenticatedNote: note1)
		let request2 = LeaveRequest(authenticatedNote: note2)
		let request3 = LeaveRequest(authenticatedNote: note3)
		let request4 = LeaveRequest(authenticatedNote: nil)

		XCTAssertEqual(request1, request2)
		XCTAssertNotEqual(request1, request3)
		XCTAssertNotEqual(request1, request4)
	}

	func testLeaveRequestCodable() throws {
		let original = LeaveRequest(authenticatedNote: Data("test".utf8))

		let encoder = JSONEncoder()
		let data = try encoder.encode(original)

		let decoder = JSONDecoder()
		let decoded = try decoder.decode(LeaveRequest.self, from: data)

		XCTAssertEqual(original, decoded)
	}

	func testContentTypeLeaveRequestValues() {
		XCTAssertEqual(ContentTypeLeaveRequest.authorityID, "xmtp.org")
		XCTAssertEqual(ContentTypeLeaveRequest.typeID, "leave_request")
		XCTAssertEqual(ContentTypeLeaveRequest.versionMajor, 1)
		XCTAssertEqual(ContentTypeLeaveRequest.versionMinor, 0)
	}

	// MARK: - Integration Tests

	func testLeaveRequestMessageIsDecodedProperly() async throws {
		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		// Alix creates a group with Bo
		let alixGroup = try await alixClient.conversations.newGroup(
			with: [boClient.inboxID]
		)

		// Bo syncs and gets the group
		_ = try await boClient.conversations.syncAllConversations()
		let boGroup = try XCTUnwrap(
			boClient.conversations.findGroup(groupId: alixGroup.id)
		)

		// Bo leaves the group - this creates a LeaveRequest message
		try await boGroup.leaveGroup()

		// Alix syncs to receive the leave request
		try await alixGroup.sync()

		// Wait for the admin worker to process the removal
		try await Task.sleep(nanoseconds: Self.adminWorkerDelayNs)

		// Alix syncs again to get the messages
		try await alixGroup.sync()

		// Get enriched messages - this goes through DecodedMessageV2 which decodes LeaveRequest
		let messages = try await alixGroup.enrichedMessages()

		// Find the messages from Bo (the leave request)
		let boMessages = messages.filter { $0.senderInboxId == boClient.inboxID }
		XCTAssertTrue(!boMessages.isEmpty, "Bo should have sent at least one message")

		// Find the leave request message by checking for LeaveRequest content type
		let leaveRequestMessage = boMessages.first { msg in
			msg.contentTypeId == ContentTypeLeaveRequest
		}

		XCTAssertNotNil(
			leaveRequestMessage,
			"LeaveRequest message should be properly decoded with correct content type"
		)

		// Verify we can decode the content as LeaveRequest
		if let leaveMsg = leaveRequestMessage {
			let leaveRequest: LeaveRequest = try leaveMsg.content()
			XCTAssertNotNil(leaveRequest, "Should be able to decode LeaveRequest content")
		}

		try fixtures.cleanUpDatabases()
	}

	func testLeaveRequestContentTypeIsCorrect() async throws {
		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		// Alix creates a group with Bo
		let alixGroup = try await alixClient.conversations.newGroup(
			with: [boClient.inboxID]
		)

		// Bo syncs and gets the group
		_ = try await boClient.conversations.syncAllConversations()
		let boGroup = try XCTUnwrap(
			boClient.conversations.findGroup(groupId: alixGroup.id)
		)

		// Bo leaves the group
		try await boGroup.leaveGroup()

		// Alix syncs
		try await alixGroup.sync()
		try await Task.sleep(nanoseconds: Self.adminWorkerDelayNs)
		try await alixGroup.sync()

		// Get enriched messages
		let messages = try await alixGroup.enrichedMessages()

		// Get all content types present
		let contentTypes = Set(messages.map(\.contentTypeId))

		// Verify LeaveRequest content type format
		if let leaveRequestType = contentTypes.first(where: { $0.typeID == "leave_request" }) {
			XCTAssertEqual(leaveRequestType.authorityID, "xmtp.org")
			XCTAssertEqual(leaveRequestType.typeID, "leave_request")
			XCTAssertEqual(leaveRequestType.versionMajor, 1)
			XCTAssertEqual(leaveRequestType.versionMinor, 0)
		}

		try fixtures.cleanUpDatabases()
	}

	func testLeaveRequestFallbackText() async throws {
		let fixtures = try await fixtures()
		let alixClient = try XCTUnwrap(fixtures.alixClient)
		let boClient = try XCTUnwrap(fixtures.boClient)

		// Alix creates a group with Bo
		let alixGroup = try await alixClient.conversations.newGroup(
			with: [boClient.inboxID]
		)

		// Bo syncs and gets the group
		_ = try await boClient.conversations.syncAllConversations()
		let boGroup = try XCTUnwrap(
			boClient.conversations.findGroup(groupId: alixGroup.id)
		)

		// Bo leaves the group
		try await boGroup.leaveGroup()

		// Alix syncs
		try await alixGroup.sync()
		try await Task.sleep(nanoseconds: Self.adminWorkerDelayNs)
		try await alixGroup.sync()

		// Get enriched messages
		let messages = try await alixGroup.enrichedMessages()

		// Find the leave request message
		let leaveRequestMessage = messages.first { msg in
			msg.contentTypeId == ContentTypeLeaveRequest
		}

		// If we found a leave request message, verify the body falls back properly
		if let leaveMsg = leaveRequestMessage {
			// The body property should return the fallback text when content isn't a String
			let body = try leaveMsg.body
			XCTAssertFalse(body.isEmpty, "Leave request should have fallback text")
		}

		try fixtures.cleanUpDatabases()
	}
}
