import CryptoKit
import XCTest
import XMTPTestHelpers

@testable import XMTPiOS

@available(iOS 16, *)
class ConversationTests: XCTestCase {
	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	func testCanFindConversationByTopic() async throws {
		let fixtures = try await fixtures()

		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])
		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		let sameDm = try await fixtures.boClient.conversations.findConversationByTopic(
			topic: dm.topic
		)
		let sameGroup = try await fixtures.boClient.conversations.findConversationByTopic(
			topic: group.topic
		)

		XCTAssertEqual(group.id, sameGroup?.id)
		XCTAssertEqual(dm.id, sameDm?.id)
		try fixtures.cleanUpDatabases()
	}

	func testCanListConversations() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])

		let convoCount = try await fixtures.boClient.conversations
			.list().count
		let dmCount = try await fixtures.boClient.conversations.listDms().count
		let groupCount = try await fixtures.boClient.conversations.listGroups()
			.count
		XCTAssertEqual(convoCount, 2)
		XCTAssertEqual(dmCount, 1)
		XCTAssertEqual(groupCount, 1)

		try await fixtures.caroClient.conversations.sync()
		let convoCount2 = try await fixtures.caroClient.conversations.list()
			.count
		let groupCount2 = try await fixtures.caroClient.conversations
			.listGroups().count
		XCTAssertEqual(convoCount2, 2)
		XCTAssertEqual(groupCount2, 1)
		try fixtures.cleanUpDatabases()
	}

	func testCanListConversationsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])

		let convoCount = try await fixtures.boClient.conversations
			.list().count
		let convoCountConsent = try await fixtures.boClient.conversations
			.list(consentStates: [.allowed]).count

		XCTAssertEqual(convoCount, 2)
		XCTAssertEqual(convoCountConsent, 2)

		try await group.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.list(consentStates: [.allowed]).count
		let convoCountDenied = try await fixtures.boClient.conversations
			.list(consentStates: [.denied]).count
		let convoCountCombined = try await fixtures.boClient.conversations
			.list(consentStates: [.denied, .allowed]).count

		XCTAssertEqual(convoCountAllowed, 1)
		XCTAssertEqual(convoCountDenied, 1)
		XCTAssertEqual(convoCountCombined, 2)
		try fixtures.cleanUpDatabases()
	}

	func testCanSyncAllConversationsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])

		let convoCount = try await fixtures.boClient.conversations
			.syncAllConversations().numEligible
		let convoCountConsent = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.allowed]).numEligible

		XCTAssertEqual(convoCount, 3)
		XCTAssertEqual(convoCountConsent, 3)

		try await group.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.allowed]).numEligible
		let convoCountDenied = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.denied]).numEligible
		let convoCountCombined = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.denied, .allowed]).numEligible

		XCTAssertEqual(convoCountAllowed, 2)
		XCTAssertEqual(convoCountDenied, 2)
		XCTAssertEqual(convoCountCombined, 3)
		try fixtures.cleanUpDatabases()
	}

	func testCanListConversationsOrder() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let group1 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caroClient.inboxID]
		)
		let group2 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caroClient.inboxID]
		)

		_ = try await dm.send(content: "Howdy")
		_ = try await group2.send(content: "Howdy")
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let conversations = try await fixtures.boClient.conversations
			.list()

		XCTAssertEqual(conversations.count, 3)
		XCTAssertEqual(
			conversations.map(\.id), [group2.id, dm.id, group1.id]
		)
		try fixtures.cleanUpDatabases()
	}

	func testCanStreamConversations() async throws {
		let fixtures = try await fixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations.stream() {
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])
		_ = try await fixtures.boClient.conversations.newConversation(
			with: fixtures.alixClient.inboxID
		)
		_ = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		await fulfillment(of: [expectation1], timeout: 3)
		try fixtures.cleanUpDatabases()
	}

	func testCanStreamAllMessages() async throws {
		let fixtures = try await fixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2
		let convo = try await fixtures.boClient.conversations.newConversation(
			with: fixtures.alixClient.inboxID
		)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])
		let dm = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		try await fixtures.alixClient.conversations.sync()
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations
				.streamAllMessages()
			{
				expectation1.fulfill()
			}
		}

		_ = try await group.send(content: "hi")
		_ = try await convo.send(content: "hi")
		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
		try fixtures.cleanUpDatabases()
	}

	func testReturnsAllHMACKeys() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let opts = ClientOptions(
			api: ClientOptions.Api(env: .local, isSecure: XMTPEnvironment.local.isSecure),
			dbEncryptionKey: key
		)
		let fixtures = try await fixtures()
		var conversations: [Conversation] = []
		for _ in 0 ..< 5 {
			let account = try PrivateKey.generate()
			let client = try await Client.create(
				account: account, options: opts
			)
			do {
				let newConversation = try await fixtures.alixClient
					.conversations
					.newConversation(
						with: client.inboxID
					)
				conversations.append(newConversation)
			} catch {
				print("Error creating conversation: \(error)")
			}
		}
		let hmacKeys = try await fixtures.alixClient.conversations.getHmacKeys()
		let topics = hmacKeys.hmacKeys.keys
		for conversation in conversations {
			XCTAssertTrue(topics.contains(conversation.topic))
		}
		try fixtures.cleanUpDatabases()
	}

	func testMessagesDontDisappear() async throws {
		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [
				fixtures.boClient.inboxID,
			]
		)

		_ = try await fixtures.alixClient.conversations.syncAllConversations()

		_ = try await alixGroup.send(content: "hello world")

		let alixMessages = try await alixGroup.messages()
		XCTAssertEqual(alixMessages.count, 2)

		try await Task.sleep(nanoseconds: 1_000_000_000) // 1 seconds

		try await alixGroup.sync()

		let messages_2 = try await alixGroup.messages()

		XCTAssertEqual(messages_2.count, 2)

		try fixtures.cleanUpDatabases()
	}

	func testStreamsAndMessages() async throws {
		var messages: [String] = []
		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [
				fixtures.caroClient.inboxID, fixtures.boClient.inboxID,
			]
		)

		let caroGroup2 = try await fixtures.caroClient.conversations.newGroup(
			with: [
				fixtures.alixClient.inboxID, fixtures.boClient.inboxID,
			]
		)

		_ = try await fixtures.alixClient.conversations.syncAllConversations()
		_ = try await fixtures.caroClient.conversations.syncAllConversations()
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let boGroupResult = try await fixtures.boClient.conversations.findGroup(
			groupId: alixGroup.id
		)
		let boGroup = try XCTUnwrap(boGroupResult)
		let caroGroupResult = try await fixtures.caroClient.conversations.findGroup(
			groupId: alixGroup.id
		)
		let caroGroup = try XCTUnwrap(caroGroupResult)
		let boGroup2Result = try await fixtures.boClient.conversations.findGroup(
			groupId: caroGroup2.id
		)
		let boGroup2 = try XCTUnwrap(boGroup2Result)
		let alixGroup2Result = try await fixtures.alixClient.conversations.findGroup(
			groupId: caroGroup2.id
		)
		let alixGroup2 = try XCTUnwrap(alixGroup2Result)

		// Start listening for messages
		let caroTask = Task {
			print("Caro is listening...")
			do {
				for try await message in await fixtures.caroClient.conversations
					.streamAllMessages()
				{
					try messages.append(message.body)
					try print("Caro received: \(message.body)")

					if messages.count >= 90 { break }
				}
			} catch {
				print("Error while streaming messages: \(error)")
			}
		}

		try await Task.sleep(nanoseconds: 1_000_000_000) // 1 second delay

		// Simulate message sending in parallel
		await withThrowingTaskGroup(of: Void.self) { taskGroup in
			taskGroup.addTask {
				print("Alix is sending messages...")
				for i in 0 ..< 20 {
					let message = "Alix Message \(i)"
					_ = try await alixGroup.send(content: message)
					_ = try await alixGroup2.send(content: message)
					print("Alix sent: \(message)")
				}
			}

			taskGroup.addTask {
				print("Bo is sending messages...")
				for i in 0 ..< 10 {
					let message = "Bo Message \(i)"
					_ = try await boGroup.send(content: message)
					_ = try await boGroup2.send(content: message)
					print("Bo sent: \(message)")
				}
			}

			taskGroup.addTask {
				print("Davon is sending spam groups...")
				for i in 0 ..< 10 {
					let spamMessage = "Davon Spam Message \(i)"
					let group = try await fixtures.davonClient.conversations
						.newGroup(
							with: [fixtures.caroClient.inboxID]
						)
					_ = try await group.send(content: spamMessage)
					print("Davon spam: \(spamMessage)")
				}
			}

			taskGroup.addTask {
				print("Caro is sending messages...")
				for i in 0 ..< 10 {
					let message = "Caro Message \(i)"
					_ = try await caroGroup.send(content: message)
					_ = try await caroGroup2.send(content: message)
					print("Caro sent: \(message)")
				}
			}
		}

		// Wait a bit to ensure all messages are processed
		try await Task.sleep(nanoseconds: 5_000_000_000) // 2 seconds delay

		caroTask.cancel()

		// This test seems to fail with some random number between 87, 88, or 89, even with increased delay
		XCTAssertEqual(messages.count, 90)
		let caroMessagesCount = try await caroGroup.messages().count
		XCTAssertEqual(caroMessagesCount, 41)

		try await boGroup.sync()
		try await alixGroup.sync()
		try await caroGroup.sync()

		let boMessagesCount = try await boGroup.messages().count
		let alixMessagesCount = try await alixGroup.messages().count
		let caroMessagesCountAfterSync = try await caroGroup.messages().count

		XCTAssertEqual(boMessagesCount, 41)
		XCTAssertEqual(alixMessagesCount, 41)
		XCTAssertEqual(caroMessagesCountAfterSync, 41)
		try fixtures.cleanUpDatabases()
	}

	func testCanCreateOptimisticGroup() async throws {
		let fixtures = try await fixtures()

		let optimisticGroup = try await fixtures.boClient.conversations.newGroupOptimistic(
			groupName: "Testing"
		)

		XCTAssertEqual(try optimisticGroup.name(), "Testing")

		_ = try await optimisticGroup.prepareMessage(content: "testing")
		let messages = try await optimisticGroup.messages()
		XCTAssertEqual(messages.count, 1)

		_ = try await optimisticGroup.addMembers(inboxIds: [fixtures.alixClient.inboxID])
		try await optimisticGroup.sync()
		try await optimisticGroup.publishMessages()

		let messagesUpdated = try await optimisticGroup.messages()
		let members = try await optimisticGroup.members
		let name = try optimisticGroup.name()
		XCTAssertEqual(messagesUpdated.count, 2)
		XCTAssertEqual(members.count, 2)
		XCTAssertEqual(name, "Testing")
		try fixtures.cleanUpDatabases()
	}

	func testCanStreamAllMessagesFilterConsent() async throws {
		let fixtures = try await fixtures()

		// Create groups and conversations
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])
		let conversation = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let blockedGroup = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alixClient.inboxID,
		])
		let blockedConversation = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		// Block some conversations
		try await blockedGroup.updateConsentState(state: .denied)
		try await blockedConversation.updateConsentState(state: .denied)
		try await fixtures.boClient.conversations.sync()

		// Collect messages
		var allMessages: [DecodedMessage] = []
		let expectation = XCTestExpectation(description: "received allowed messages")
		expectation.expectedFulfillmentCount = 2

		// Start streaming
		let streamTask = Task {
			for try await message in await fixtures.boClient.conversations.streamAllMessages(
				consentStates: [.allowed]
			) {
				allMessages.append(message)
				expectation.fulfill()

				if allMessages.count >= 2 {
					break
				}
			}
		}

		// Wait a bit before sending messages
		try await Task.sleep(nanoseconds: 1_000_000_000) // 1 second

		// Send messages to all conversations
		_ = try await group.send(content: "hi")
		_ = try await conversation.send(content: "hi")
		_ = try await blockedGroup.send(content: "hi")
		_ = try await blockedConversation.send(content: "hi")

		// Wait for expectation to be fulfilled or timeout
		await fulfillment(of: [expectation], timeout: 3)

		// Cancel streaming task
		streamTask.cancel()

		// Verify we only received messages from allowed conversations
		XCTAssertEqual(allMessages.count, 2)
		try fixtures.cleanUpDatabases()
	}

	func testReturnsAllTopics() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let opts = ClientOptions(
			api: ClientOptions.Api(env: .local, isSecure: XMTPEnvironment.local.isSecure),
			dbEncryptionKey: key
		)

		// Create a new private key for Eri
		let eriWallet = try PrivateKey.generate()

		// Create first client for Eri
		let eriClient = try await Client.create(
			account: eriWallet,
			options: opts
		)

		let fixtures = try await fixtures()

		// Create first DM
		let dm1 = try await eriClient.conversations.findOrCreateDm(
			with: fixtures.boClient.inboxID
		)

		// Create a group
		_ = try await fixtures.boClient.conversations.newGroup(
			with: [eriClient.inboxID]
		)

		// Create a second client with the same key
		let dbPath = FileManager.default.temporaryDirectory.appendingPathComponent(
			UUID().uuidString
		).path
		var opts2 = opts
		opts2.dbDirectory = dbPath

		let eriClient2 = try await Client.create(
			account: eriWallet,
			options: opts2
		)

		// Create a second DM using the second client
		_ = try await eriClient2.conversations.findOrCreateDm(
			with: fixtures.boClient.inboxID
		)

		// Sync all the clients
		_ = try await fixtures.boClient.conversations.syncAllConversations()
		_ = try await eriClient2.conversations.syncAllConversations()
		_ = try await eriClient.conversations.syncAllConversations()

		// Get all the topics and HMAC keys
		let allTopics = try await eriClient.conversations.allPushTopics()
		let conversations = try await eriClient.conversations.list()
		let allHmacKeys = try await eriClient.conversations.getHmacKeys()
		let dmHmacKeys = try dm1.getHmacKeys()
		let dmTopics = try await dm1.getPushTopics()

		// Assertions
		XCTAssertEqual(allTopics.count, 3)
		XCTAssertEqual(conversations.count, 2)

		let hmacTopics = allHmacKeys.hmacKeys.keys
		for topic in allTopics {
			XCTAssertTrue(hmacTopics.contains(topic))
		}

		XCTAssertEqual(dmTopics.count, 2)
		XCTAssertTrue(Set(allTopics).isSuperset(of: Set(dmTopics)))

		let dmHmacTopics = dmHmacKeys.hmacKeys.keys
		for topic in dmTopics {
			XCTAssertTrue(dmHmacTopics.contains(topic))
		}
		try fixtures.cleanUpDatabases()
	}

	func testCanListConversationsAndCheckCommitLogForkStatus() async throws {
		let fixtures = try await fixtures()

		_ = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		_ = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])

		try await fixtures.caroClient.conversations.sync()
		let caroConversations = try await fixtures.caroClient.conversations.list()

		XCTAssertEqual(caroConversations.count, 2)

		var numForkStatusUnknown = 0
		var numForkStatusForked = 0
		var numForkStatusNotForked = 0

		for conversation in caroConversations {
			let forkStatus = conversation.commitLogForkStatus()
			switch forkStatus {
			case .forked:
				numForkStatusForked += 1
			case .notForked:
				numForkStatusNotForked += 1
			case .unknown:
				numForkStatusUnknown += 1
			}
		}

		// Right now worker runs every 5 minutes so we'd need to wait that long to verify not forked
		XCTAssertEqual(numForkStatusForked, 0)
		XCTAssertEqual(numForkStatusNotForked, 0)
		XCTAssertEqual(numForkStatusUnknown, 2)

		try fixtures.cleanUpDatabases()
	}

	func testDeleteMessage() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		let msgID = try await dm.send(content: "This will be deleted")

		let originalNumberOfMessages = try await dm.messages().count
		try fixtures.boClient.conversations.deleteMessageLocally(messageId: msgID)

		let newMessageCount = try await dm.messages().count
		XCTAssertEqual(newMessageCount, originalNumberOfMessages - 1)
	}

	func testCountMessages() async throws {
		let fixtures = try await fixtures()

		// Create a group and a DM
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])
		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		// Initially both should have 0 messages (or 1 for group with membership change)
		let initialGroupCount = try group.countMessages()
		let initialDmCount = try dm.countMessages()

		// Group might have 1 initial membership change message
		XCTAssertTrue(initialGroupCount == 0 || initialGroupCount == 1)
		XCTAssertEqual(initialDmCount, 0)

		// Send messages to the group
		_ = try await group.send(content: "Group message 1")
		_ = try await group.send(content: "Group message 2")
		_ = try await group.send(content: "Group message 3")

		// Send messages to the DM
		_ = try await dm.send(content: "DM message 1")
		_ = try await dm.send(content: "DM message 2")

		// Count messages in both conversations
		let groupCount = try group.countMessages()
		let dmCount = try dm.countMessages()

		// Verify counts (group may have +1 for membership change)
		XCTAssertTrue(groupCount == 3 || groupCount == 4)
		XCTAssertEqual(dmCount, 2)

		// Test counting with delivery status filter
		let publishedGroupCount = try group.countMessages(deliveryStatus: .published)
		let publishedDmCount = try dm.countMessages(deliveryStatus: .published)

		// All sent messages should be published
		XCTAssertTrue(publishedGroupCount == 3 || publishedGroupCount == 4)
		XCTAssertEqual(publishedDmCount, 2)

		// Test counting with time-based filters
		let now = Int64(Date().millisecondsSinceEpoch)
		let futureGroupCount = try group.countMessages(afterNs: now * 1_000_000)
		let futureDmCount = try dm.countMessages(afterNs: now * 1_000_000)

		// No messages should be after current time
		XCTAssertEqual(futureGroupCount, 0)
		XCTAssertEqual(futureDmCount, 0)

		try fixtures.cleanUpDatabases()
	}

	func testMessagesWithExcludedContentTypes() async throws {
		let fixtures = try await fixtures()
		Client.register(codec: ReactionCodec())

		// Create a group
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
		])

		// Send different types of messages
		_ = try await group.send(content: "Text message 1")
		_ = try await group.send(content: "Text message 2")

		// Send a reaction
		let textMessageId = try await group.send(content: "Message to react to")
		_ = try await group.send(
			content: Reaction(
				reference: textMessageId,
				action: .added,
				content: "👍",
				schema: .unicode
			),
			options: SendOptions(contentType: ContentTypeReaction)
		)

		// Wait a bit for messages to sync
		try await Task.sleep(nanoseconds: 100_000_000)
		try await group.sync()

		// Get all messages
		let allMessages = try await group.messages()

		// Get messages excluding reactions
		let messagesWithoutReactions = try await group.messages(
			excludeContentTypes: [.reaction]
		)

		// Should have fewer messages when excluding reactions
		XCTAssertGreaterThan(allMessages.count, messagesWithoutReactions.count)

		// Verify no reactions in the filtered list
		for message in messagesWithoutReactions {
			XCTAssertNotEqual(try message.encodedContent.type.typeID, "reaction")
		}

		try fixtures.cleanUpDatabases()
	}

	func testCountMessagesWithExcludedContentTypes() async throws {
		let fixtures = try await fixtures()
		Client.register(codec: ReactionCodec())

		// Create a DM
		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		// Send different types of messages
		_ = try await dm.send(content: "Text message 1")
		_ = try await dm.send(content: "Text message 2")
		_ = try await dm.send(content: "Text message 3")

		// Send reactions
		let textMessageId = try await dm.send(content: "Message to react to")
		_ = try await dm.send(
			content: Reaction(
				reference: textMessageId,
				action: .added,
				content: "👍",
				schema: .unicode
			),
			options: SendOptions(contentType: ContentTypeReaction)
		)
		_ = try await dm.send(
			content: Reaction(
				reference: textMessageId,
				action: .added,
				content: "❤️",
				schema: .unicode
			),
			options: SendOptions(contentType: ContentTypeReaction)
		)

		// Wait a bit for messages to sync
		try await Task.sleep(nanoseconds: 100_000_000)
		try await dm.sync()

		// Count all messages
		let totalCount = try dm.countMessages()

		// Count messages excluding reactions
		let countWithoutReactions = try dm.countMessages(
			excludeContentTypes: [.reaction]
		)

		// Should have 6 total (4 text + 2 reactions)
		XCTAssertEqual(totalCount, 6)

		// Should have 4 without reactions
		XCTAssertEqual(countWithoutReactions, 4)

		// Verify the difference equals the number of reactions
		XCTAssertEqual(totalCount - countWithoutReactions, 2)

		try fixtures.cleanUpDatabases()
	}
}
