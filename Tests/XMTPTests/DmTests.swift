import LibXMTP
import XCTest
import XMTPTestHelpers

@testable import XMTPiOS

@available(iOS 16, *)
class DmTests: XCTestCase {

	func testCanFindDmByInboxId() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)

		let caroDm = try await fixtures.boClient.conversations.findDmByInboxId(
			inboxId: fixtures.caroClient.inboxID)
		let alixDm = try await fixtures.boClient.conversations.findDmByInboxId(
			inboxId: fixtures.alixClient.inboxID)

		XCTAssertNil(alixDm)
		XCTAssertEqual(caroDm?.id, dm.id)
	}

	func testCanFindDmByAddress() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)

		let caroDm = try await fixtures.boClient.conversations.findDmByAddress(
			address: fixtures.caroClient.address)
		let alixDm = try await fixtures.boClient.conversations.findDmByAddress(
			address: fixtures.alixClient.address)

		XCTAssertNil(alixDm)
		XCTAssertEqual(caroDm?.id, dm.id)
	}

	func testCanCreateADm() async throws {
		let fixtures = try await fixtures()

		let convo1 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		try await fixtures.alixClient.conversations.sync()
		let sameConvo1 = try await fixtures.alixClient.conversations
			.findOrCreateDm(with: fixtures.bo.walletAddress)
		XCTAssertEqual(convo1.id, sameConvo1.id)
	}

	func testCanCreateADmWithInboxId() async throws {
		let fixtures = try await fixtures()

		let convo1 = try await fixtures.boClient.conversations
			.findOrCreateDmWithInboxId(
				with: fixtures.alixClient.inboxID)
		try await fixtures.alixClient.conversations.sync()
		let sameConvo1 = try await fixtures.alixClient.conversations
			.findOrCreateDmWithInboxId(with: fixtures.boClient.inboxID)
		XCTAssertEqual(convo1.id, sameConvo1.id)
	}

	func testCanListDmMembers() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		let members = try await dm.members
		XCTAssertEqual(members.count, 2)

		let peer = try dm.peerInboxId
		XCTAssertEqual(peer, fixtures.alixClient.inboxID)
	}

	func testCannotStartGroupWithSelf() async throws {
		let fixtures = try await fixtures()

		await assertThrowsAsyncError(
			try await fixtures.alixClient.conversations.findOrCreateDm(
				with: fixtures.alix.address)
		)
	}

	func testCannotStartGroupWithNonRegisteredIdentity() async throws {
		let fixtures = try await fixtures()
		let nonRegistered = try PrivateKey.generate()

		await assertThrowsAsyncError(
			try await fixtures.alixClient.conversations.findOrCreateDm(
				with: nonRegistered.address)
		)
	}

	func testDmStartsWithAllowedState() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		_ = try await dm.send(content: "howdy")
		_ = try await dm.send(content: "gm")
		try await dm.sync()

		let dmState = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(dmState, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
	}

	func testCanListDmsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let dm2 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caro.walletAddress
		])

		let convoCount = try await fixtures.boClient.conversations
			.listDms().count
		let convoCountConsent = try await fixtures.boClient.conversations
			.listDms(consentStates: [.allowed]).count

		XCTAssertEqual(convoCount, 2)
		XCTAssertEqual(convoCountConsent, 2)

		try await dm2.updateConsentState(state: .denied)

		let convoCountAllowed = try await fixtures.boClient.conversations
			.listDms(consentStates: [.allowed]).count
		let convoCountDenied = try await fixtures.boClient.conversations
			.listDms(consentStates: [.denied]).count
		let convoCountCombined = try await fixtures.boClient.conversations
			.listDms(consentStates: [.denied, .allowed]).count

		XCTAssertEqual(convoCountAllowed, 1)
		XCTAssertEqual(convoCountDenied, 1)
		XCTAssertEqual(convoCountCombined, 2)
	}

	func testCanListConversationsOrder() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caro.walletAddress)
		let dm2 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		let group2 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caro.walletAddress])

		_ = try await dm.send(content: "Howdy")
		_ = try await dm2.send(content: "Howdy")
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let conversations = try await fixtures.boClient.conversations
			.listDms()
		XCTAssertEqual(conversations.count, 2)
		XCTAssertEqual(
			try conversations.map { try $0.id }, [dm2.id, dm.id])
	}

	func testCanSendMessageToDm() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		_ = try await dm.send(content: "howdy")
		let messageId = try await dm.send(content: "gm")
		try await dm.sync()

		let firstMessage = try await dm.messages().first!
		XCTAssertEqual(try firstMessage.body, "gm")
		XCTAssertEqual(firstMessage.id, messageId)
		XCTAssertEqual(firstMessage.deliveryStatus, .published)
		let messages = try await dm.messages()
		XCTAssertEqual(messages.count, 3)

		try await fixtures.alixClient.conversations.sync()
		let sameDm = try await fixtures.alixClient.conversations.listDms().last!
		try await sameDm.sync()

		let sameMessages = try await sameDm.messages()
		XCTAssertEqual(sameMessages.count, 2)
		XCTAssertEqual(try sameMessages.first!.body, "gm")
	}

	func testCanStreamDmMessages() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		try await fixtures.alixClient.conversations.sync()

		let expectation1 = XCTestExpectation(description: "got a message")
		expectation1.expectedFulfillmentCount = 1

		Task(priority: .userInitiated) {
			for try await _ in dm.streamMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await dm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}

	func testCanStreamDms() async throws {
		let fixtures = try await fixtures()

		let expectation1 = XCTestExpectation(description: "got a group")
		expectation1.expectedFulfillmentCount = 1

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations
				.stream(type: .dms)
			{
				expectation1.fulfill()
			}
		}

		_ = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.alix.address
		])
		_ = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alix.address)

		await fulfillment(of: [expectation1], timeout: 3)
	}

	func testCanStreamAllDmMessages() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)
		try await fixtures.alixClient.conversations.sync()

		let expectation1 = XCTestExpectation(description: "got a message")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations
				.streamAllMessages(type: .dms)
			{
				expectation1.fulfill()
			}
		}

		_ = try await dm.send(content: "hi")
		let caroDm = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.address)
		_ = try await caroDm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
	}

	func testDmConsent() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress)

		let isDm = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isDm, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)

		try await fixtures.boClient.preferences.setConsentState(
			entries: [
				ConsentRecord(
					value: dm.id, entryType: .conversation_id,
					consentType: .denied)
			])
		let isDenied = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isDenied, .denied)
		XCTAssertEqual(try dm.consentState(), .denied)

		try await dm.updateConsentState(state: .allowed)
		let isAllowed = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isAllowed, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
	}
	
	func testDmDisappearingMessages() async throws {
		let fixtures = try await fixtures()

		let initialSettings = DisappearingMessageSettings(
			disappearStartingAtNs: 1_000_000_000,
			retentionDurationInNs: 1_000_000_000  // 1s duration
		)

		// Create group with disappearing messages enabled
		let boDm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alix.walletAddress,
			disappearingMessageSettings: initialSettings
		)
		_ = try await boDm.send(content: "howdy")
		_ = try await fixtures.alixClient.conversations.syncAllConversations()
		
		let alixDm = try await fixtures.alixClient.conversations.findDmByInboxId(inboxId: fixtures.boClient.inboxID)

		let boGroupMessagesCount = try await boDm.messages().count
		let alixGroupMessagesCount = try await alixDm?.messages().count
		let boGroupSettings = boDm.disappearingMessageSettings

		// Validate messages exist and settings are applied
		XCTAssertEqual(boGroupMessagesCount, 2)  // memberAdd howdy
		XCTAssertEqual(alixGroupMessagesCount, 1)  // howdy
		XCTAssertNotNil(boGroupSettings)

		try await Task.sleep(nanoseconds: 5_000_000_000)  // Sleep for 5 seconds

		let boGroupMessagesAfterSleep = try await boDm.messages().count
		let alixGroupMessagesAfterSleep = try await alixDm?.messages().count

		// Validate messages are deleted
		XCTAssertEqual(boGroupMessagesAfterSleep, 1)
		XCTAssertEqual(alixGroupMessagesAfterSleep, 0)

		// Set message disappearing settings to nil
		try await boDm.updateDisappearingMessageSettings(nil)
		try await boDm.sync()
		try await alixDm?.sync()

		let boGroupSettingsAfterNil = boDm.disappearingMessageSettings
		let alixGroupSettingsAfterNil = alixDm?.disappearingMessageSettings

		XCTAssertNil(boGroupSettingsAfterNil)
		XCTAssertNil(alixGroupSettingsAfterNil)
		XCTAssertFalse(try boDm.isDisappearingMessagesEnabled())
		XCTAssertFalse(try alixDm!.isDisappearingMessagesEnabled())

		// Send messages after disabling disappearing settings
		_ = try await boDm.send(
			content: "message after disabling disappearing")
		_ = try await alixDm?.send(
			content: "another message after disabling")
		try await boDm.sync()

		try await Task.sleep(nanoseconds: 5_000_000_000)  // Sleep for 5 seconds

		let boGroupMessagesPersist = try await boDm.messages().count
		let alixGroupMessagesPersist = try await alixDm?.messages().count

		// Ensure messages persist
		XCTAssertEqual(boGroupMessagesPersist, 5)  // memberAdd settings 1, settings 2, boMessage, alixMessage
		XCTAssertEqual(alixGroupMessagesPersist, 4)  // settings 1, settings 2, boMessage, alixMessage

		// Re-enable disappearing messages
		let updatedSettings = await DisappearingMessageSettings(
			disappearStartingAtNs: try boDm.messages().first!.sentAtNs
				+ 1_000_000_000,  // 1s from now
			retentionDurationInNs: 1_000_000_000  // 2s duration
		)
		try await boDm.updateDisappearingMessageSettings(updatedSettings)
		try await boDm.sync()
		try await alixDm?.sync()
		try await Task.sleep(nanoseconds: 1_000_000_000)  // Sleep for 1 second

		let boGroupUpdatedSettings = boDm.disappearingMessageSettings
		let alixGroupUpdatedSettings = alixDm?.disappearingMessageSettings

		XCTAssertEqual(
			boGroupUpdatedSettings!.retentionDurationInNs,
			updatedSettings.retentionDurationInNs)
		XCTAssertEqual(
			alixGroupUpdatedSettings!.retentionDurationInNs,
			updatedSettings.retentionDurationInNs)

		// Send new messages
		_ = try await boDm.send(content: "this will disappear soon")
		_ = try await alixDm?.send(content: "so will this")
		try await boDm.sync()

		let boGroupMessagesAfterNewSend = try await boDm.messages().count
		let alixGroupMessagesAfterNewSend = try await alixDm?.messages()
			.count

		XCTAssertEqual(boGroupMessagesAfterNewSend, 9)
		XCTAssertEqual(alixGroupMessagesAfterNewSend, 8)

		try await Task.sleep(nanoseconds: 6_000_000_000)  // Sleep for 6 seconds to let messages disappear

		let boGroupMessagesFinal = try await boDm.messages().count
		let alixGroupMessagesFinal = try await alixDm?.messages().count

		// Validate messages were deleted
		XCTAssertEqual(boGroupMessagesFinal, 7)
		XCTAssertEqual(alixGroupMessagesFinal, 6)

		let boGroupFinalSettings = boDm.disappearingMessageSettings
		let alixGroupFinalSettings = alixDm?.disappearingMessageSettings

		XCTAssertEqual(
			boGroupFinalSettings!.retentionDurationInNs,
			updatedSettings.retentionDurationInNs)
		XCTAssertEqual(
			alixGroupFinalSettings!.retentionDurationInNs,
			updatedSettings.retentionDurationInNs)
		XCTAssert(try boDm.isDisappearingMessagesEnabled())
		XCTAssert(try alixDm!.isDisappearingMessagesEnabled())
	}
}
