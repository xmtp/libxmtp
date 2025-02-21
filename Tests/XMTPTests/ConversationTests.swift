import CryptoKit
import LibXMTP
import XCTest
import XMTPTestHelpers

@testable import XMTPiOS

@available(iOS 16, *)
class ConversationTests: XCTestCase {
	func testCanFindConversationByTopic() async throws {
		let fixtures = try await fixtures()

		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caro.walletAddress
		])
		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)

		let sameDm = try await fixtures.boClient.conversations.findConversationByTopic(
			topic: dm.topic)
		let sameGroup = try await fixtures.boClient.conversations.findConversationByTopic(
			topic: group.topic)

		XCTAssertEqual(group.id, sameGroup?.id)
		XCTAssertEqual(dm.id, sameDm?.id)
	}

	func testCanListConversations() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caro.walletAddress
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
	}

	func testCanListConversationsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caro.walletAddress
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
	}

	func testCanSyncAllConversationsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caro.walletAddress
		])

		let convoCount = try await fixtures.boClient.conversations
			.syncAllConversations()
		let convoCountConsent = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.allowed])

		XCTAssertEqual(convoCount, 3)
		XCTAssertEqual(convoCountConsent, 3)

		try await group.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.allowed])
		let convoCountDenied = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.denied])
		let convoCountCombined = try await fixtures.boClient.conversations
			.syncAllConversations(consentStates: [.denied, .allowed])

		XCTAssertEqual(convoCountAllowed, 2)
		XCTAssertEqual(convoCountDenied, 2)
		XCTAssertEqual(convoCountCombined, 3)
	}

	func testCanListConversationsOrder() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let group1 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caro.walletAddress])
		let group2 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caro.walletAddress])

		_ = try await dm.send(content: "Howdy")
		_ = try await group2.send(content: "Howdy")
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let conversations = try await fixtures.boClient.conversations
			.list()

		XCTAssertEqual(conversations.count, 3)
		XCTAssertEqual(
			conversations.map { $0.id }, [group2.id, dm.id, group1.id])
	}

	func testCanStreamConversations() async throws {
		let fixtures = try await fixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations.stream()
			{
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alix.address
		])
		_ = try await fixtures.boClient.conversations.newConversation(
			with: fixtures.alix.address)
		_ = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alix.address)

		await fulfillment(of: [expectation1], timeout: 3)
	}

	func testCanStreamAllMessages() async throws {
		let fixtures = try await fixtures()

		let expectation1 = XCTestExpectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2
		let convo = try await fixtures.boClient.conversations.newConversation(
			with: fixtures.alix.address)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alix.address
		])
		let dm = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alix.address)

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
	}

	func testReturnsAllHMACKeys() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let opts = ClientOptions(
			api: ClientOptions.Api(env: .local, isSecure: false),
			dbEncryptionKey: key)
		let fixtures = try await fixtures()
		var conversations: [Conversation] = []
		for _ in 0..<5 {
			let account = try PrivateKey.generate()
			let client = try await Client.create(
				account: account, options: opts)
			do {
				let newConversation = try await fixtures.alixClient
					.conversations
					.newConversation(
						with: client.address
					)
				conversations.append(newConversation)
			} catch {
				print("Error creating conversation: \(error)")
			}
		}
		let hmacKeys = try await fixtures.alixClient.conversations.getHmacKeys()
		let topics = hmacKeys.hmacKeys.keys
		conversations.forEach { conversation in
			XCTAssertTrue(topics.contains(conversation.topic))
		}
	}

	func testStreamsAndMessages() async throws {
		var messages: [String] = []
		let fixtures = try await fixtures()

		let alixGroup = try await fixtures.alixClient.conversations.newGroup(
			with: [
				fixtures.caroClient.address, fixtures.boClient.address,
			])

		let caroGroup2 = try await fixtures.caroClient.conversations.newGroup(
			with: [
				fixtures.alixClient.address, fixtures.boClient.address,
			])

		_ = try await fixtures.alixClient.conversations.syncAllConversations()
		_ = try await fixtures.caroClient.conversations.syncAllConversations()
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let boGroup = try await fixtures.boClient.conversations.findGroup(groupId: alixGroup.id)!
		let caroGroup = try await fixtures.caroClient.conversations.findGroup(
			groupId: alixGroup.id)!
		let boGroup2 = try await fixtures.boClient.conversations.findGroup(groupId: caroGroup2.id)!
		let alixGroup2 = try await fixtures.alixClient.conversations.findGroup(
			groupId: caroGroup2.id)!

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

		try await Task.sleep(nanoseconds: 1_000_000_000)  // 1 second delay

		// Simulate message sending in parallel
		await withThrowingTaskGroup(of: Void.self) { taskGroup in
			taskGroup.addTask {
				print("Alix is sending messages...")
				for i in 0..<20 {
					let message = "Alix Message \(i)"
					_ = try await alixGroup.send(content: message)
					_ = try await alixGroup2.send(content: message)
					print("Alix sent: \(message)")
				}
			}

			taskGroup.addTask {
				print("Bo is sending messages...")
				for i in 0..<10 {
					let message = "Bo Message \(i)"
					_ = try await boGroup.send(content: message)
					_ = try await boGroup2.send(content: message)
					print("Bo sent: \(message)")
				}
			}

			taskGroup.addTask {
				print("Davon is sending spam groups...")
				for i in 0..<10 {
					let spamMessage = "Davon Spam Message \(i)"
					let group = try await fixtures.davonClient.conversations
						.newGroup(
							with: [fixtures.caroClient.address]
						)
					_ = try await group.send(content: spamMessage)
					print("Davon spam: \(spamMessage)")
				}
			}

			taskGroup.addTask {
				print("Caro is sending messages...")
				for i in 0..<10 {
					let message = "Caro Message \(i)"
					_ = try await caroGroup.send(content: message)
					_ = try await caroGroup2.send(content: message)
					print("Caro sent: \(message)")
				}
			}
		}

		// Wait a bit to ensure all messages are processed
		try await Task.sleep(nanoseconds: 2_000_000_000)  // 2 seconds delay

		caroTask.cancel()

		XCTAssertEqual(messages.count, 90)
		let caroMessagesCount = try await caroGroup.messages().count
		XCTAssertEqual(caroMessagesCount, 40)

		try await boGroup.sync()
		try await alixGroup.sync()
		try await caroGroup.sync()

		let boMessagesCount = try await boGroup.messages().count
		let alixMessagesCount = try await alixGroup.messages().count
		let caroMessagesCountAfterSync = try await caroGroup.messages().count

		XCTAssertEqual(boMessagesCount, 40)
		XCTAssertEqual(alixMessagesCount, 41)
		XCTAssertEqual(caroMessagesCountAfterSync, 40)
	}
}
