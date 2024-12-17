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

		let sameDm = try await fixtures.boClient.findConversationByTopic(
			topic: dm.topic)
		let sameGroup = try await fixtures.boClient.findConversationByTopic(
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
			.list(consentState: .allowed).count

		XCTAssertEqual(convoCount, 2)
		XCTAssertEqual(convoCountConsent, 2)

		try await group.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.list(consentState: .allowed).count
		let convoCountDenied = try await fixtures.boClient.conversations
			.list(consentState: .denied).count

		XCTAssertEqual(convoCountAllowed, 1)
		XCTAssertEqual(convoCountDenied, 1)
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
			.syncAllConversations(consentState: .allowed)

		XCTAssertEqual(convoCount, 2)
		XCTAssertEqual(convoCountConsent, 2)

		try await group.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.syncAllConversations(consentState: .allowed)
		let convoCountDenied = try await fixtures.boClient.conversations
			.syncAllConversations(consentState: .denied)

		XCTAssertEqual(convoCountAllowed, 1)
		XCTAssertEqual(convoCountDenied, 1)
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
		let conversationsOrdered = try await fixtures.boClient.conversations
			.list(order: .lastMessage)

		XCTAssertEqual(conversations.count, 3)
		XCTAssertEqual(conversationsOrdered.count, 3)

		XCTAssertEqual(
			conversations.map { $0.id }, [dm.id, group1.id, group2.id])
		XCTAssertEqual(
			conversationsOrdered.map { $0.id },
			[group2.id, dm.id, group1.id])
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

	func testSyncConsent() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		var alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let dm = try await alixClient.conversations.findOrCreateDm(
			with: fixtures.bo.walletAddress)
		try await dm.updateConsentState(state: .denied)
		XCTAssertEqual(try dm.consentState(), .denied)

		try await fixtures.boClient.conversations.sync()
		let boDm = try await fixtures.boClient.findConversation(conversationId: dm.id)

		var alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db2"
			)
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await fixtures.boClient.conversations.sync()
		try await boDm?.sync()
		try await alixClient2.preferences.syncConsent()
		try await alixClient.conversations.syncAllConversations()
		sleep(2)
		try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let dm2 = try await alixClient2.findConversation(conversationId: dm.id) {
			XCTAssertEqual(try dm2.consentState(), .denied)

			try await alixClient2.preferences.setConsentState(
				entries: [
					ConsentRecord(
						value: dm2.id,
						entryType: .conversation_id,
						consentType: .allowed
					)
				]
			)
			let convoState = try await alixClient2.preferences
				.conversationState(
					conversationId: dm2.id)
			XCTAssertEqual(convoState, .allowed)
			XCTAssertEqual(try dm2.consentState(), .allowed)
		}
	}
	
	func testStreamConsent() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let alixGroup = try await alixClient.conversations.newGroup(with: [fixtures.bo.walletAddress])

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db2"
			)
		)
		
		try await alixGroup.send(content: "Hello")
		try await alixClient.conversations.syncAllConversations()
		try await alixClient2.conversations.syncAllConversations()
		let alixGroup2 = try alixClient2.findGroup(groupId: alixGroup.id)!

		var consentList = [ConsentRecord]()
		let expectation = XCTestExpectation(description: "Stream Consent")
		expectation.expectedFulfillmentCount = 3

		Task(priority: .userInitiated) {
			for try await entry in await alixClient.preferences.streamConsent() {
				consentList.append(entry)
				expectation.fulfill()
			}
		}
		sleep(1)
		try await alixGroup2.updateConsentState(state: .denied)
		let dm = try await alixClient2.conversations.newConversation(with: fixtures.caro.walletAddress)
		try await dm.updateConsentState(state: .denied)

		sleep(5)
		try await alixClient.conversations.syncAllConversations()
		try await alixClient2.conversations.syncAllConversations()

		await fulfillment(of: [expectation], timeout: 3)
		print(consentList)
		XCTAssertEqual(try alixGroup.consentState(), .denied)
	}


}
