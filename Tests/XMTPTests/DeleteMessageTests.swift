import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class DeleteMessageTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	// MARK: - Group Tests

	func testSenderCanDeleteOwnMessageInGroup() async throws {
		let fixtures = try await fixtures()

		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])

		let messageId = try await group.send(content: "Message to delete")

		try await group.sync()
		let messagesBefore = try await group.messages()
		let textMessagesBefore = messagesBefore.filter { (try? $0.encodedContent.type.typeID) == "text" }
		XCTAssertEqual(textMessagesBefore.count, 1)

		let deleteMessageId = try await group.deleteMessage(messageId: messageId)
		XCTAssertFalse(deleteMessageId.isEmpty)

		try await group.sync()
		let enrichedMessages = try await group.enrichedMessages()
		let deletedMessage = enrichedMessages.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		let deletedContent: DeletedMessage = try XCTUnwrap(deletedMessage?.content())
		XCTAssertEqual(deletedContent.deletedBy, .sender)

		try fixtures.cleanUpDatabases()
	}

	func testDeletedMessageSyncsToOtherClients() async throws {
		let fixtures = try await fixtures()

		let boGroup = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])

		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try fixtures.alixClient.conversations.findGroup(groupId: boGroup.id)
		XCTAssertNotNil(alixGroup)

		let messageId = try await boGroup.send(content: "Message to delete")
		try await boGroup.sync()
		try await alixGroup?.sync()

		let alixMessagesBefore = try await alixGroup?.enrichedMessages()
		let originalMessage = alixMessagesBefore?.first { $0.id == messageId }
		XCTAssertNotNil(originalMessage)
		XCTAssertEqual(originalMessage?.contentTypeId.typeID, "text")

		_ = try await boGroup.deleteMessage(messageId: messageId)
		try await boGroup.sync()
		try await alixGroup?.sync()

		let alixMessagesAfter = try await alixGroup?.enrichedMessages()
		let deletedMessage = alixMessagesAfter?.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		let deletedContent: DeletedMessage = try XCTUnwrap(deletedMessage?.content())
		XCTAssertEqual(deletedContent.deletedBy, .sender)

		try fixtures.cleanUpDatabases()
	}

	func testAdminCanDeleteOtherUsersMessageInGroup() async throws {
		let fixtures = try await fixtures()

		let boGroup = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])

		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try fixtures.alixClient.conversations.findGroup(groupId: boGroup.id)
		XCTAssertNotNil(alixGroup)

		let messageId = try try await XCTUnwrap(alixGroup?.send(content: "Alix's message"))
		try await boGroup.sync()
		try await alixGroup?.sync()

		_ = try await boGroup.deleteMessage(messageId: messageId)
		try await boGroup.sync()

		let enrichedMessages = try await boGroup.enrichedMessages()
		let deletedMessage = enrichedMessages.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		let deletedContent: DeletedMessage = try XCTUnwrap(deletedMessage?.content())
		if case let .admin(inboxId) = deletedContent.deletedBy {
			XCTAssertEqual(inboxId, fixtures.boClient.inboxID)
		} else {
			XCTFail("Expected admin deletion")
		}

		try fixtures.cleanUpDatabases()
	}

	// MARK: - DM Tests

	func testSenderCanDeleteOwnMessageInDM() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		let messageId = try await dm.send(content: "Message to delete")

		try await dm.sync()
		let messagesBefore = try await dm.messages()
		let textMessagesBefore = messagesBefore.filter { (try? $0.encodedContent.type.typeID) == "text" }
		XCTAssertEqual(textMessagesBefore.count, 1)

		let deleteMessageId = try await dm.deleteMessage(messageId: messageId)
		XCTAssertFalse(deleteMessageId.isEmpty)

		try await dm.sync()
		let enrichedMessages = try await dm.enrichedMessages()
		let deletedMessage = enrichedMessages.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		let deletedContent: DeletedMessage = try XCTUnwrap(deletedMessage?.content())
		XCTAssertEqual(deletedContent.deletedBy, .sender)

		try fixtures.cleanUpDatabases()
	}

	func testDeletedMessageSyncsToOtherClientInDM() async throws {
		let fixtures = try await fixtures()

		let boDm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		try await fixtures.alixClient.conversations.sync()
		let alixDm = try fixtures.alixClient.conversations.findDmByInboxId(
			inboxId: fixtures.boClient.inboxID
		)
		XCTAssertNotNil(alixDm)

		let messageId = try await boDm.send(content: "Message to delete")
		try await boDm.sync()
		try await alixDm?.sync()

		let alixMessagesBefore = try await alixDm?.enrichedMessages()
		let originalMessage = alixMessagesBefore?.first { $0.id == messageId }
		XCTAssertNotNil(originalMessage)
		XCTAssertEqual(originalMessage?.contentTypeId.typeID, "text")

		_ = try await boDm.deleteMessage(messageId: messageId)
		try await boDm.sync()
		try await alixDm?.sync()

		let alixMessagesAfter = try await alixDm?.enrichedMessages()
		let deletedMessage = alixMessagesAfter?.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Conversation Wrapper Tests

	func testDeleteMessageViaConversationWrapper() async throws {
		let fixtures = try await fixtures()

		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])
		let conversation = Conversation.group(group)

		let messageId = try await conversation.send(content: "Message to delete")
		try await conversation.sync()

		_ = try await conversation.deleteMessage(messageId: messageId)
		try await conversation.sync()

		let enrichedMessages = try await conversation.enrichedMessages()
		let deletedMessage = enrichedMessages.first { $0.id == messageId }

		XCTAssertNotNil(deletedMessage)
		XCTAssertEqual(deletedMessage?.contentTypeId.typeID, "deletedMessage")

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Streaming Tests

	func testStreamingDeletedMessages() async throws {
		let fixtures = try await fixtures()

		let boGroup = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])

		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try fixtures.alixClient.conversations.findGroup(groupId: boGroup.id)
		XCTAssertNotNil(alixGroup)

		var streamedMessages: [DecodedMessage] = []
		let expectation = XCTestExpectation(description: "received delete message")

		let streamTask = Task {
			for try await message in alixGroup!.streamMessages() {
				streamedMessages.append(message)
				let contentType = try message.encodedContent.type.typeID
				if contentType == "deleteMessage" {
					expectation.fulfill()
					break
				}
			}
		}

		try await Task.sleep(nanoseconds: 500_000_000)

		let messageId = try await boGroup.send(content: "Message to delete")
		try await boGroup.sync()

		try await Task.sleep(nanoseconds: 500_000_000)

		_ = try await boGroup.deleteMessage(messageId: messageId)
		try await boGroup.sync()

		await fulfillment(of: [expectation], timeout: 5)

		streamTask.cancel()

		let deleteMessages = streamedMessages.filter {
			(try? $0.encodedContent.type.typeID) == "deleteMessage"
		}
		XCTAssertGreaterThanOrEqual(deleteMessages.count, 1)

		try fixtures.cleanUpDatabases()
	}
}
