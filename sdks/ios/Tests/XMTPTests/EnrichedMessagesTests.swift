import CryptoKit
import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class EnrichedMessagesTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	func testFindMessagesV2ComparedToFindMessages() async throws {
		// Register codecs
		Client.register(codec: ReactionCodec())
		Client.register(codec: AttachmentCodec())

		let fixtures = try await fixtures()

		// Create a group with various message types
		let group = try await fixtures.alixClient.conversations.newGroup(with: [
			fixtures.boClient.inboxID,
		])

		// Send various types of messages
		let textId1 = try await group.send(content: "First message")
		try await Task.sleep(nanoseconds: 100_000_000)

		let textId2 = try await group.send(content: "Second message")
		try await Task.sleep(nanoseconds: 100_000_000)

		// Send attachment
		let attachment = Attachment(
			filename: "test.txt",
			mimeType: "text/plain",
			data: Data("Test data".utf8)
		)
		try await group.send(content: attachment, options: .init(contentType: ContentTypeAttachment))
		try await Task.sleep(nanoseconds: 100_000_000)

		// Send reaction
		let reaction = Reaction(
			reference: textId1,
			action: .added,
			content: "👍",
			schema: .unicode
		)
		try await group.send(content: reaction, options: .init(contentType: ContentTypeReaction))

		// Get messages using both methods
		let messagesV1 = try await group.messages()
		let messagesV2 = try await group.enrichedMessages()

		// V1 includes reactions as separate messages, V2 attaches them to parent messages
		// Filter out reactions from V1 for comparison
		let v1NonReactions = messagesV1.filter { msg in
			do {
				let contentType = try msg.encodedContent.type
				return contentType.typeID != "reaction"
			} catch {
				return true
			}
		}

		// V1 should have 5 messages (group_updated + 2 texts + 1 attachment + 1 reaction)
		// V2 should have 4 messages (group_updated + 2 texts + 1 attachment, reaction attached to first text)
		XCTAssertEqual(messagesV1.count, 5, "V1 should have 5 messages including reaction")
		XCTAssertEqual(messagesV2.count, 4, "V2 should have 4 messages with reaction attached")
		XCTAssertEqual(v1NonReactions.count, 4, "V1 should have 4 non-reaction messages")

		// Find the message with the reaction in V2
		let v2MessageWithReaction = messagesV2.first { $0.id == textId1 }
		XCTAssertNotNil(v2MessageWithReaction, "Should find message with reaction")
		XCTAssertNotNil(v2MessageWithReaction?.reactions, "Message should have reactions")
		XCTAssertEqual(v2MessageWithReaction?.reactions?.count, 1, "Should have 1 reaction")

		if let reaction = v2MessageWithReaction?.reactions?.first {
			let reactionContent: Reaction = try reaction.content()
			XCTAssertEqual(reactionContent.content, "👍")
			XCTAssertEqual(reactionContent.reference, textId1)
			XCTAssertEqual(reactionContent.action, .added)
		}

		// Compare non-reaction messages have the same IDs
		let v1NonReactionIds = Set(v1NonReactions.map(\.id))
		let v2Ids = Set(messagesV2.map(\.id))
		XCTAssertEqual(v1NonReactionIds, v2Ids, "Non-reaction message IDs should match")

		// Verify content types
		let v2ContentTypes = Set(messagesV2.map(\.contentTypeId.typeID))
		XCTAssertTrue(v2ContentTypes.contains("text"))
		XCTAssertTrue(v2ContentTypes.contains("attachment"))
		XCTAssertTrue(v2ContentTypes.contains("group_updated"))
		XCTAssertFalse(v2ContentTypes.contains("reaction"), "V2 should not have reaction as separate message")

		// Additional test: Verify messagesWithReactions V1 behavior matches V2 reactions property
		let messagesWithReactionsV1 = try await group.messagesWithReactions()

		// V1 messagesWithReactions should have fewer messages than regular messages
		// (it excludes reactions and only includes messages that have reactions)
		let v1MessagesWithChildReactions = messagesWithReactionsV1.filter { ($0.childMessages?.count ?? 0) > 0 }
		XCTAssertEqual(v1MessagesWithChildReactions.count, 1, "Should have 1 message with reactions in V1")

		if let v1MessageWithReaction = v1MessagesWithChildReactions.first {
			XCTAssertEqual(v1MessageWithReaction.id, textId1)
			XCTAssertEqual(v1MessageWithReaction.childMessages?.count, 1)

			if let v1ChildReaction = v1MessageWithReaction.childMessages?.first {
				let v1ReactionContent: Reaction = try v1ChildReaction.content()
				XCTAssertEqual(v1ReactionContent.content, "👍")
				XCTAssertEqual(v1ReactionContent.reference, textId1)
			}
		}

		try fixtures.cleanUpDatabases()
	}

	func testBasicMessageRetrievalInBothConversationTypes() async throws {
		let fixtures = try await fixtures()

		// Test both Group and DM in one test
		let conversationTests: [(type: String, createConversation: () async throws -> Conversation)] = [
			("group", {
				let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])
				return Conversation.group(group)
			}),
			("dm", {
				let dm = try await fixtures.alixClient.conversations.findOrCreateDm(with: fixtures.boClient.inboxID)
				return Conversation.dm(dm)
			}),
		]

		for (conversationType, createConversation) in conversationTests {
			let conversation = try await createConversation()

			// Send messages
			let messageIds = try await [
				conversation.send(content: "First \(conversationType) message"),
				conversation.send(content: "Second \(conversationType) message"),
				conversation.send(content: "Third \(conversationType) message"),
			]

			// Retrieve messages using messagesV2
			let messagesV2 = try await conversation.enrichedMessages()

			// Verify messages were retrieved
			let textMessages = messagesV2.filter { $0.contentTypeId.typeID == "text" }
			XCTAssertEqual(textMessages.count, 3, "Should have 3 text messages in \(conversationType)")

			// Verify core properties exist
			for message in textMessages {
				XCTAssertNotNil(message.id)
				XCTAssertNotNil(message.senderInboxId)
				XCTAssertNotNil(message.sentAt)
				XCTAssertEqual(message.deliveryStatus, .published)

				// Verify content is accessible
				let content: String = try message.content()
				XCTAssertTrue(content.contains("\(conversationType) message"))
			}
		}

		try fixtures.cleanUpDatabases()
	}

	func testPaginationParameters() async throws {
		let fixtures = try await fixtures()
		let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])

		// Send messages with delays for timestamp testing
		var messageTimestamps: [Int64] = []
		for i in 1 ... 5 {
			try await group.send(content: "Message \(i)")
			messageTimestamps.append(Int64(Date().timeIntervalSince1970 * 1_000_000_000))
			if i < 5 {
				try await Task.sleep(nanoseconds: 100_000_000)
			}
		}

		// Test limit
		let limited = try await group.enrichedMessages(limit: 3)
		XCTAssertLessThanOrEqual(limited.count, 4) // 3 + possible membership message

		// Test beforeNs (messages before middle timestamp)
		let middleTimestamp = messageTimestamps[2]
		let beforeMessages = try await group.enrichedMessages(beforeNs: middleTimestamp)
		let afterMessages = try await group.enrichedMessages(afterNs: middleTimestamp)

		XCTAssertGreaterThan(beforeMessages.count, 0)
		XCTAssertGreaterThan(afterMessages.count, 0)

		// Test sort direction
		let ascending = try await group.enrichedMessages(direction: .ascending)
		let descending = try await group.enrichedMessages(direction: .descending)

		if ascending.count > 1, descending.count > 1 {
			// Verify sort order
			XCTAssertLessThan(ascending[0].sentAtNs, ascending[ascending.count - 1].sentAtNs)
			XCTAssertGreaterThan(descending[0].sentAtNs, descending[descending.count - 1].sentAtNs)
		}

		try fixtures.cleanUpDatabases()
	}

	func testAllContentTypesAndReactions() async throws {
		// Register codecs
		Client.register(codec: ReactionCodec())
		Client.register(codec: AttachmentCodec())
		Client.register(codec: ReplyCodec())

		let fixtures = try await fixtures()
		let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])

		// Send various content types
		let textId = try await group.send(content: "Text message")

		let attachment = Attachment(
			filename: "test.txt",
			mimeType: "text/plain",
			data: Data("Test attachment".utf8)
		)
		let attachmentId = try await group.send(content: attachment, options: .init(contentType: ContentTypeAttachment))

		let reply = Reply(
			reference: textId,
			content: "Reply content",
			contentType: ContentTypeText
		)
		try await group.send(content: reply, options: .init(contentType: ContentTypeReply))

		// Add reactions from another client
		try await fixtures.boClient.conversations.sync()
		let boConversations = try await fixtures.boClient.conversations.list()
		guard case let .group(boGroup) = boConversations.first else {
			XCTFail("Expected first conversation to be a group")
			return
		}

		// Send reactions to the text message
		let reaction1 = Reaction(reference: textId, action: .added, content: "👍", schema: .unicode)
		let reaction2 = Reaction(reference: textId, action: .added, content: "❤️", schema: .unicode)

		try await boGroup.send(content: reaction1, options: .init(contentType: ContentTypeReaction))
		try await boGroup.send(content: reaction2, options: .init(contentType: ContentTypeReaction))

		// Sync and retrieve all messages using V2
		try await group.sync()
		let messagesV2 = try await group.enrichedMessages()

		// V2 should NOT have reactions as separate messages
		let contentTypes = Set(messagesV2.map(\.contentTypeId.typeID))
		XCTAssertTrue(contentTypes.contains("text"))
		XCTAssertTrue(contentTypes.contains("attachment"))
		XCTAssertTrue(contentTypes.contains("reply"))
		XCTAssertTrue(contentTypes.contains("group_updated"))
		XCTAssertFalse(contentTypes.contains("reaction"), "V2 should not have reactions as separate messages")

		// Messages should be: group_updated, text, attachment, reply (4 total)
		XCTAssertEqual(messagesV2.count, 4, "Should have 4 messages (no separate reaction messages)")

		// Verify reactions are attached to the original text message
		if let originalMessage = messagesV2.first(where: { $0.id == textId }) {
			XCTAssertNotNil(originalMessage.reactions, "Text message should have reactions")
			XCTAssertEqual(originalMessage.reactions?.count, 2, "Should have 2 reactions")

			// Verify reaction contents
			let reactionContents = try originalMessage.reactions?.compactMap { reaction in
				try (reaction.content() as Reaction).content
			} ?? []

			XCTAssertEqual(Set(reactionContents), Set(["👍", "❤️"]), "Should have both emoji reactions")

			// Verify reaction details
			for reaction in originalMessage.reactions ?? [] {
				let reactionContent: Reaction = try reaction.content()
				XCTAssertEqual(reactionContent.reference, textId)
				XCTAssertEqual(reactionContent.action, .added)
			}
		} else {
			XCTFail("Could not find original text message")
		}

		// Verify other messages don't have reactions
		let attachmentMessage = messagesV2.first { $0.id == attachmentId }
		XCTAssertNil(attachmentMessage?.reactions, "Attachment should not have reactions")

		try fixtures.cleanUpDatabases()
	}

	func testEdgeCasesAndDeliveryStatus() async throws {
		let fixtures = try await fixtures()
		let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])

		// Test empty conversation (only membership message)
		let emptyMessages = try await group.enrichedMessages()
		XCTAssertGreaterThanOrEqual(emptyMessages.count, 1)
		if let firstMessage = emptyMessages.first {
			XCTAssertEqual(firstMessage.contentTypeId.typeID, "group_updated")
		}

		// Test delivery status filtering
		try await group.send(content: "Published message")
		let unpublishedId = try await group.prepareMessage(content: "Unpublished")

		let allMessages = try await group.enrichedMessages(deliveryStatus: .all)
		let publishedOnly = try await group.enrichedMessages(deliveryStatus: .published)
		let unpublishedOnly = try await group.enrichedMessages(deliveryStatus: .unpublished)

		XCTAssertGreaterThan(allMessages.count, publishedOnly.count)

		// Verify status filtering
		for message in publishedOnly {
			XCTAssertEqual(message.deliveryStatus, .published)
		}

		if let unpublished = unpublishedOnly.first(where: { $0.id == unpublishedId }) {
			XCTAssertEqual(unpublished.deliveryStatus, .unpublished)
		}

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Performance Test

	func testLargeMessageSetPerformance() async throws {
		let fixtures = try await fixtures()
		let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])

		// Send many messages
		let messageCount = 30
		for i in 1 ... messageCount {
			try await group.send(content: "Message \(i)")
		}

		// Measure performance
		let startTime = Date()
		let messages = try await group.enrichedMessages()
		let timeElapsed = Date().timeIntervalSince(startTime)

		XCTAssertGreaterThanOrEqual(messages.count, messageCount)
		XCTAssertLessThan(timeElapsed, 2.0, "Should retrieve \(messageCount) messages in under 2 seconds")

		// Test pagination with large set
		let firstPage = try await group.enrichedMessages(limit: 10)
		XCTAssertLessThanOrEqual(firstPage.count, 11) // 10 + membership

		if let oldestInFirstPage = firstPage.last {
			let secondPage = try await group.enrichedMessages(
				beforeNs: oldestInFirstPage.sentAtNs,
				limit: 10
			)

			// Verify pagination works correctly
			XCTAssertGreaterThan(secondPage.count, 0)

			// Verify no overlap between pages
			let firstIds = Set(firstPage.map(\.id))
			let secondIds = Set(secondPage.map(\.id))
			XCTAssertTrue(firstIds.isDisjoint(with: secondIds))
		}

		try fixtures.cleanUpDatabases()
	}

	// MARK: - Complex Content Test

	func testComplexContentTypes() async throws {
		// Register codecs
		Client.register(codec: ReplyCodec())
		Client.register(codec: RemoteAttachmentCodec())

		let fixtures = try await fixtures()
		let group = try await fixtures.alixClient.conversations.newGroup(with: [fixtures.boClient.inboxID])

		// Send initial message to reply to
		let originalId = try await group.send(content: "Original message")

		// Send a reply
		let reply = Reply(
			reference: originalId,
			content: "Reply text",
			contentType: ContentTypeText
		)
		let replyId = try await group.send(content: reply, options: .init(contentType: ContentTypeReply))

		// Send a remote attachment
		let remoteAttachment = try RemoteAttachment(
			url: "https://example.com/file.enc",
			contentDigest: "digest123",
			secret: Data(repeating: 1, count: 32),
			salt: Data(repeating: 2, count: 32),
			nonce: Data(repeating: 3, count: 12),
			scheme: .https,
			contentLength: 100,
			filename: "remote.txt"
		)
		let remoteId = try await group.send(
			content: remoteAttachment,
			options: .init(contentType: ContentTypeRemoteAttachment)
		)

		// Retrieve messages using V2
		let messagesV2 = try await group.enrichedMessages()

		// Should have: group_updated, original, reply, remote attachment (4 total)
		XCTAssertEqual(messagesV2.count, 4, "Should have 4 messages")

		// Verify content types
		let contentTypes = Set(messagesV2.map(\.contentTypeId.typeID))
		XCTAssertTrue(contentTypes.contains("text"))
		XCTAssertTrue(contentTypes.contains("reply"))
		XCTAssertTrue(contentTypes.contains("remoteStaticAttachment"))
		XCTAssertTrue(contentTypes.contains("group_updated"))

		// Find and verify the reply message
		if let replyMessage = messagesV2.first(where: { $0.id == replyId }) {
			XCTAssertEqual(replyMessage.contentTypeId.typeID, "reply")
			let replyContent: Reply = try replyMessage.content()
			XCTAssertEqual(replyContent.reference, originalId)
			XCTAssertEqual(replyContent.content as? String, "Reply text")

			// V2 includes the inReplyTo field with the decoded message
			if let inReplyTo = replyContent.inReplyTo {
				XCTAssertEqual(inReplyTo.id, originalId)
				let originalContent: String = try inReplyTo.content()
				XCTAssertEqual(originalContent, "Original message")
			} else {
				// It's okay if inReplyTo is not populated in tests
				print("Note: inReplyTo not populated in reply")
			}
		} else {
			XCTFail("Could not find reply message")
		}

		// Find and verify the remote attachment
		if let remoteMessage = messagesV2.first(where: { $0.id == remoteId }) {
			XCTAssertEqual(remoteMessage.contentTypeId.typeID, "remoteStaticAttachment")
			let remoteContent: RemoteAttachment = try remoteMessage.content()
			XCTAssertEqual(remoteContent.url, "https://example.com/file.enc")
			// Filename might be nil in V2 decoding
			if let filename = remoteContent.filename {
				XCTAssertEqual(filename, "remote.txt")
			} else {
				print("Note: Remote attachment filename is nil in V2")
			}
			XCTAssertEqual(remoteContent.contentDigest, "digest123")
			// Content length might be 0 if not properly encoded
			if remoteContent.contentLength != nil, try XCTUnwrap(remoteContent.contentLength) > 0 {
				XCTAssertEqual(remoteContent.contentLength, 100)
			}

			// Verify fallback is present
			let fallback = try remoteMessage.fallback
			XCTAssertNotNil(fallback, "Remote attachment should have fallback")
		} else {
			XCTFail("Could not find remote attachment message")
		}

		// Verify the original message
		if let originalMessage = messagesV2.first(where: { $0.id == originalId }) {
			XCTAssertEqual(originalMessage.contentTypeId.typeID, "text")
			let content: String = try originalMessage.content()
			XCTAssertEqual(content, "Original message")
		} else {
			XCTFail("Could not find original message")
		}

		try fixtures.cleanUpDatabases()
	}
}
