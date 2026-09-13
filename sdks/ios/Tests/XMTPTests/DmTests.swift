import XCTest
@testable import XMTPiOS
import XMTPTestHelpers

@available(iOS 16, *)
class DmTests: XCTestCase {
	func testCanFindDmByInboxId() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		let caroDm = try await fixtures.boClient.conversations.findDmByInboxId(
			inboxId: fixtures.caroClient.inboxID
		)
		let alixDm = try await fixtures.boClient.conversations.findDmByInboxId(
			inboxId: fixtures.alixClient.inboxID
		)

		XCTAssertNil(alixDm)
		XCTAssertEqual(caroDm?.id, dm.id)
		try fixtures.cleanUpDatabases()
	}

	func testCanFindDmByAddress() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)

		let caroDm = try await fixtures.boClient.conversations.findDmByIdentity(
			publicIdentity: fixtures.caro.identity
		)
		let alixDm = try await fixtures.boClient.conversations.findDmByIdentity(
			publicIdentity: fixtures.alix.identity
		)

		XCTAssertNil(alixDm)
		XCTAssertEqual(caroDm?.id, dm.id)
		try fixtures.cleanUpDatabases()
	}

	func testCanCreateADm() async throws {
		let fixtures = try await fixtures()

		let convo1 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		try await fixtures.alixClient.conversations.sync()
		let sameConvo1 = try await fixtures.alixClient.conversations
			.findOrCreateDm(with: fixtures.boClient.inboxID)
		XCTAssertEqual(convo1.id, sameConvo1.id)
		try fixtures.cleanUpDatabases()
	}

	func testCanCreateADmWithIdentity() async throws {
		let fixtures = try await fixtures()

		let convo1 = try await fixtures.boClient.conversations
			.findOrCreateDmWithIdentity(
				with: fixtures.alix.identity
			)
		try await fixtures.alixClient.conversations.sync()
		let sameConvo1 = try await fixtures.alixClient.conversations
			.newConversationWithIdentity(with: fixtures.bo.identity)
		XCTAssertEqual(convo1.id, sameConvo1.id)
		try fixtures.cleanUpDatabases()
	}

	func testCanListDmMembers() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		let members = try await dm.members
		XCTAssertEqual(members.count, 2)

		let peer = try dm.peerInboxId
		XCTAssertEqual(peer, fixtures.alixClient.inboxID)
		try fixtures.cleanUpDatabases()
	}

	func testCannotStartDmWithSelf() async throws {
		let fixtures = try await fixtures()

		try await assertThrowsAsyncError(
			await fixtures.alixClient.conversations.findOrCreateDm(
				with: fixtures.alixClient.inboxID
			)
		)
		try fixtures.cleanUpDatabases()
	}

	func testCannotStartDmWithAddressWhenExpectingInboxId() async throws {
		let fixtures = try await fixtures()

		do {
			_ = try await fixtures.boClient.conversations.newConversation(
				with: fixtures.alix.walletAddress
			)
			XCTFail("Did not throw error")
		} catch {
			if case let ClientError.invalidInboxId(message) = error {
				XCTAssertEqual(
					message.lowercased(),
					fixtures.alix.walletAddress.lowercased()
				)
			} else {
				XCTFail("Did not throw correct error")
			}
		}
		try fixtures.cleanUpDatabases()
	}

	func testCannotStartDmWithNonRegisteredIdentity() async throws {
		let fixtures = try await fixtures()
		let nonRegistered = try PrivateKey.generate()

		try await assertThrowsAsyncError(
			await fixtures.alixClient.conversations
				.findOrCreateDmWithIdentity(
					with: nonRegistered.identity
				)
		)
	}

	func testDmStartsWithAllowedState() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		_ = try await dm.send(content: "howdy")
		_ = try await dm.send(content: "gm")
		try await dm.sync()

		let dmState = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(dmState, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
		try fixtures.cleanUpDatabases()
	}

	func testCanListDmsFiltered() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let dm2 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		let group = try await fixtures.boClient.conversations.newGroup(with: [
			fixtures.caroClient.inboxID,
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
		try fixtures.cleanUpDatabases()
	}

	func testCanListConversationsOrder() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.caroClient.inboxID
		)
		let dm2 = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		let group2 = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.caroClient.inboxID]
		)

		_ = try await dm.send(content: "Howdy")
		_ = try await dm2.send(content: "Howdy")
		_ = try await fixtures.boClient.conversations.syncAllConversations()

		let conversations = try await fixtures.boClient.conversations
			.listDms()
		XCTAssertEqual(conversations.count, 2)
		XCTAssertEqual(
			try conversations.map { try $0.id }, [dm2.id, dm.id]
		)
		try fixtures.cleanUpDatabases()
	}

	func testCanSendMessageToDm() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		_ = try await dm.send(content: "howdy")
		let messageId = try await dm.send(content: "gm")
		try await dm.sync()

		let dmMessages = try await dm.messages()
		let firstMessage = try XCTUnwrap(dmMessages.first)
		XCTAssertEqual(try firstMessage.body, "gm")
		XCTAssertEqual(firstMessage.id, messageId)
		XCTAssertEqual(firstMessage.deliveryStatus, .published)
		let messages = try await dm.messages()
		XCTAssertEqual(messages.count, 3)

		try await fixtures.alixClient.conversations.sync()
		let alixDms = try await fixtures.alixClient.conversations.listDms()
		let sameDm = try XCTUnwrap(alixDms.last)
		try await sameDm.sync()

		let sameMessages = try await sameDm.messages()
		XCTAssertEqual(sameMessages.count, 3)
		XCTAssertEqual(try sameMessages.first?.body, "gm")
		try fixtures.cleanUpDatabases()
	}

	func testCanStreamDmMessages() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
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
		try fixtures.cleanUpDatabases()
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
			fixtures.alixClient.inboxID,
		])
		_ = try await fixtures.caroClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		await fulfillment(of: [expectation1], timeout: 3)
		try fixtures.cleanUpDatabases()
	}

	func testCanStreamAllDmMessages() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
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
			with: fixtures.alixClient.inboxID
		)
		_ = try await caroDm.send(content: "hi")

		await fulfillment(of: [expectation1], timeout: 3)
		try fixtures.cleanUpDatabases()
	}

	func testDmConsent() async throws {
		let fixtures = try await fixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		let isDm = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isDm, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)

		try await fixtures.boClient.preferences.setConsentState(
			entries: [
				ConsentRecord(
					value: dm.id, entryType: .conversation_id,
					consentType: .denied
				),
			]
		)
		let isDenied = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isDenied, .denied)
		XCTAssertEqual(try dm.consentState(), .denied)

		try await dm.updateConsentState(state: .allowed)
		let isAllowed = try await fixtures.boClient.preferences
			.conversationState(conversationId: dm.id)
		XCTAssertEqual(isAllowed, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
		try fixtures.cleanUpDatabases()
	}

	func testDmDisappearingMessages() async throws {
		let fixtures = try await fixtures()

		// Retention is 5s (long enough that the initial count assertion
		// below wins the race with the 1s-interval disappearing_messages
		// worker even on slow CI runners — see issue #3448).
		let initialSettings = DisappearingMessageSettings(
			disappearStartingAtNs: 1_000_000_000,
			retentionDurationInNs: 5_000_000_000 // 5s duration
		)

		// Create group with disappearing messages enabled
		let boDm = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID,
			disappearingMessageSettings: initialSettings
		)
		_ = try await boDm.send(content: "howdy")
		_ = try await fixtures.alixClient.conversations.syncAllConversations()

		let alixDm = try await fixtures.alixClient.conversations
			.findDmByInboxId(inboxId: fixtures.boClient.inboxID)

		let boGroupMessagesCount = try await boDm.messages().count
		let alixGroupMessagesCount = try await alixDm?.messages().count
		let boGroupSettings = boDm.disappearingMessageSettings

		// Validate messages exist and settings are applied
		XCTAssertEqual(boGroupMessagesCount, 2) // memberAdd howdy
		XCTAssertEqual(alixGroupMessagesCount, 2) // memberAdd howdy
		XCTAssertNotNil(boGroupSettings)

		// Sleep longer than retention (5s) + worker interval (1s) + buffer.
		try await Task.sleep(nanoseconds: 8_000_000_000) // Sleep for 8 seconds

		let boGroupMessagesAfterSleep = try await boDm.messages().count
		let alixGroupMessagesAfterSleep = try await alixDm?.messages().count

		// Validate messages are deleted
		XCTAssertEqual(boGroupMessagesAfterSleep, 1)
		XCTAssertEqual(alixGroupMessagesAfterSleep, 1)

		// Set message disappearing settings to nil
		try await boDm.updateDisappearingMessageSettings(nil)
		try await boDm.sync()
		try await alixDm?.sync()

		let boGroupSettingsAfterNil = boDm.disappearingMessageSettings
		let alixGroupSettingsAfterNil = alixDm?.disappearingMessageSettings

		XCTAssertNil(boGroupSettingsAfterNil)
		XCTAssertNil(alixGroupSettingsAfterNil)
		XCTAssertFalse(try boDm.isDisappearingMessagesEnabled())
		XCTAssertFalse(try XCTUnwrap(alixDm?.isDisappearingMessagesEnabled()))

		// Send messages after disabling disappearing settings
		_ = try await boDm.send(
			content: "message after disabling disappearing"
		)
		_ = try await alixDm?.send(
			content: "another message after disabling"
		)
		try await boDm.sync()

		try await Task.sleep(nanoseconds: 5_000_000_000) // Sleep for 5 seconds

		let boGroupMessagesPersist = try await boDm.messages().count
		let alixGroupMessagesPersist = try await alixDm?.messages().count

		// Ensure messages persist
		XCTAssertEqual(boGroupMessagesPersist, 5) // memberAdd settings 1, settings 2, boMessage, alixMessage
		XCTAssertEqual(alixGroupMessagesPersist, 5) // memberAdd settings 1, settings 2, boMessage, alixMessage

		// Re-enable disappearing messages.
		// Retention is 5s (long enough that the count==9 assertion below
		// wins the race with the 1s-interval disappearing_messages worker
		// even on slow CI runners — see issue #3505, same root cause as
		// the Phase 1 fix in #3448/#3451).
		let boDmMessages = try await boDm.messages()
		let updatedSettings = try DisappearingMessageSettings(
			disappearStartingAtNs: XCTUnwrap(boDmMessages.first?.sentAtNs)
				+ 1_000_000_000, // disappearStartingAtNs offset; does not gate deletion
			retentionDurationInNs: 5_000_000_000 // 5s duration
		)
		try await boDm.updateDisappearingMessageSettings(updatedSettings)
		try await boDm.sync()
		try await alixDm?.sync()
		try await Task.sleep(nanoseconds: 1_000_000_000) // Sleep for 1 second

		let boGroupUpdatedSettings = boDm.disappearingMessageSettings
		let alixGroupUpdatedSettings = alixDm?.disappearingMessageSettings

		XCTAssertEqual(
			boGroupUpdatedSettings?.retentionDurationInNs,
			updatedSettings.retentionDurationInNs
		)
		XCTAssertEqual(
			alixGroupUpdatedSettings?.retentionDurationInNs,
			updatedSettings.retentionDurationInNs
		)

		// Send new messages
		_ = try await boDm.send(content: "this will disappear soon")
		_ = try await alixDm?.send(content: "so will this")
		try await boDm.sync()

		let boGroupMessagesAfterNewSend = try await boDm.messages().count
		let alixGroupMessagesAfterNewSend = try await alixDm?.messages()
			.count

		XCTAssertEqual(boGroupMessagesAfterNewSend, 9)
		XCTAssertEqual(alixGroupMessagesAfterNewSend, 9)

		// Sleep longer than retention (5s) + worker interval (1s) + buffer — see issue #3505.
		try await Task.sleep(nanoseconds: 8_000_000_000) // Sleep for 8 seconds to let messages disappear

		let boGroupMessagesFinal = try await boDm.messages().count
		let alixGroupMessagesFinal = try await alixDm?.messages().count

		// Validate messages were deleted
		XCTAssertEqual(boGroupMessagesFinal, 7)
		XCTAssertEqual(alixGroupMessagesFinal, 7)

		let boGroupFinalSettings = boDm.disappearingMessageSettings
		let alixGroupFinalSettings = alixDm?.disappearingMessageSettings

		XCTAssertEqual(
			boGroupFinalSettings?.retentionDurationInNs,
			updatedSettings.retentionDurationInNs
		)
		XCTAssertEqual(
			alixGroupFinalSettings?.retentionDurationInNs,
			updatedSettings.retentionDurationInNs
		)
		XCTAssert(try boDm.isDisappearingMessagesEnabled())
		XCTAssert(try XCTUnwrap(alixDm?.isDisappearingMessagesEnabled()))
		try fixtures.cleanUpDatabases()
	}

	func testCanSuccessfullyThreadDms() async throws {
		func independentClient() async throws -> Client {
			let account = try PrivateKey.generate()
			let api = localApi()
			let dbPath = randomTempFile()
			let ffi = try await createClient(
				api: Client.connectToApiBackend(api: api),
				db: DbOptions(db: dbPath, encryptionKey: nil, maxDbPoolSize: nil, minDbPoolSize: nil),
				inboxId: generateInboxId(accountIdentifier: account.identity.ffiPrivate, nonce: 0),
				accountIdentifier: account.identity.ffiPrivate,
				nonce: 0, legacySignedPrivateKeyProto: nil, deviceSyncMode: .disabled,
				allowOffline: false, forkRecoveryOpts: nil,
				workerConfig: FfiWorkerConfig(
					defaultIntervalNs: nil, workerIntervalsNs: [], workerJittersNs: [],
					disabledWorkers: [.deviceSync, .disappearingMessages, .keyPackageCleaner, .commitLog, .taskRunner]
				),
				changeCallbacks: nil
			)
			let signatureRequest = try XCTUnwrap(ffi.signatureRequest())
			let signature = try await account.sign(signatureRequest.signatureText())
			try await signatureRequest.addEcdsaSignature(signatureBytes: signature.rawData)
			try await ffi.registerIdentity(signatureRequest: signatureRequest, visibilityConfirmationOptions: nil)
			let client = try Client(
				ffiClient: ffi, dbPath: dbPath, installationID: ffi.installationId().toHex,
				inboxID: ffi.inboxId(), environment: api.env, publicIdentity: account.identity
			)
			addTeardownBlock { try client.deleteLocalDatabase() }
			return client
		}

		// Neither client receives Welcomes before both physical DMs exist.
		let fixtures = try await (boClient: independentClient(), alixClient: independentClient())
		Client.register(codec: GroupUpdatedCodec())

		let convoBo = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)
		let convoAlix = try await fixtures.alixClient.conversations
			.findOrCreateDm(with: fixtures.boClient.inboxID)
		XCTAssertNotEqual(convoBo.id, convoAlix.id)

		let boMessageID = try await convoBo.send(content: "Bo hey")
		try await Task.sleep(nanoseconds: 5_000_000_000) // 5 seconds delay
		let alixMessageID = try await convoAlix.send(content: "Alix hey")

		var expectedApplications = [
			boMessageID: (sender: fixtures.boClient.inboxID, body: "Bo hey", group: convoBo.id),
			alixMessageID: (sender: fixtures.alixClient.inboxID, body: "Alix hey", group: convoAlix.id),
		]

		func assertHistory(_ messages: [DecodedMessage], requiredIDs: Set<String>) throws {
			XCTAssertEqual(Set(messages.map(\.id)).count, messages.count)
			let applications = messages.filter { $0.kind == .application }
			XCTAssertTrue(requiredIDs.isSubset(of: Set(applications.map(\.id))))
			let setup = messages.filter { $0.kind == .membershipChange }
			XCTAssertTrue((1 ... 2).contains(setup.count))
			XCTAssertEqual(Set(setup.map(\.conversationId)).count, setup.count)
			for message in messages {
				XCTAssertTrue([convoBo.id, convoAlix.id].contains(message.conversationId))
				if message.kind == .application {
					let expected = try XCTUnwrap(expectedApplications[message.id])
					let body: String = try message.content()
					XCTAssertEqual(body, expected.body)
					XCTAssertEqual(message.senderInboxId, expected.sender)
					XCTAssertEqual(message.conversationId, expected.group)
				} else {
					XCTAssertEqual(message.kind, .membershipChange)
					XCTAssertEqual(try message.encodedContent.type, ContentTypeGroupUpdated)
					let update: GroupUpdated = try message.content()
					var expected = GroupUpdated()
					expected.initiatedByInboxID = update.initiatedByInboxID
					var added = GroupUpdated.Inbox()
					if update.initiatedByInboxID == fixtures.boClient.inboxID {
						added.inboxID = fixtures.alixClient.inboxID
					} else {
						XCTAssertEqual(update.initiatedByInboxID, fixtures.alixClient.inboxID)
						added.inboxID = fixtures.boClient.inboxID
					}
					expected.addedInboxes = [added]
					XCTAssertEqual(update, expected)
				}
			}
		}

		// Background receipt can store either duplicate DM's Welcome before sync.
		try await assertHistory(convoBo.messages(), requiredIDs: [boMessageID])
		try await assertHistory(convoAlix.messages(), requiredIDs: [alixMessageID])

		_ = try await fixtures.boClient.conversations.syncAllConversations()
		_ = try await fixtures.alixClient.conversations.syncAllConversations()

		let boMessagesAfterSync = try await convoBo.messages()
		let alixMessagesAfterSync = try await convoAlix.messages()
		XCTAssertEqual(boMessagesAfterSync.count, 4)
		XCTAssertEqual(alixMessagesAfterSync.count, 4)
		try assertHistory(boMessagesAfterSync, requiredIDs: Set(expectedApplications.keys))
		try assertHistory(alixMessagesAfterSync, requiredIDs: Set(expectedApplications.keys))

		let sameConvoBo = try await fixtures.alixClient.conversations
			.findOrCreateDm(with: fixtures.boClient.inboxID)
		let sameConvoAlix = try await fixtures.boClient.conversations
			.findOrCreateDm(with: fixtures.alixClient.inboxID)

		let topicBoSame = try await fixtures.boClient.conversations
			.findConversationByTopic(topic: convoBo.topic)
		let topicAlixSame = try await fixtures.alixClient.conversations
			.findConversationByTopic(topic: convoAlix.topic)

		let alixConvoID = convoAlix.id
		let topicBoSameID = topicBoSame?.id
		let topicAlixSameID = topicAlixSame?.id
		let firstAlixDmID = try fixtures.alixClient.conversations
			.listDms().first?.id
		let firstBoDmID = try fixtures.boClient.conversations.listDms()
			.first?.id

		XCTAssertEqual(alixConvoID, sameConvoBo.id)
		XCTAssertEqual(alixConvoID, sameConvoAlix.id)
		XCTAssertEqual(alixConvoID, topicBoSameID)
		XCTAssertEqual(alixConvoID, topicAlixSameID)
		XCTAssertEqual(firstAlixDmID, alixConvoID)
		XCTAssertEqual(firstBoDmID, alixConvoID)

		let boMessageID2 = try await sameConvoBo.send(content: "Bo hey2")
		let alixMessageID2 = try await sameConvoAlix.send(content: "Alix hey2")
		expectedApplications[boMessageID2] = (fixtures.alixClient.inboxID, "Bo hey2", sameConvoBo.id)
		expectedApplications[alixMessageID2] = (fixtures.boClient.inboxID, "Alix hey2", sameConvoAlix.id)
		try await sameConvoAlix.sync()
		try await sameConvoBo.sync()

		let sameConvoBoMessages = try await sameConvoBo.messages()
		let sameConvoAlixMessages = try await sameConvoAlix.messages()
		XCTAssertEqual(sameConvoBoMessages.count, 6)
		XCTAssertEqual(sameConvoAlixMessages.count, 6)
		try assertHistory(sameConvoBoMessages, requiredIDs: Set(expectedApplications.keys))
		try assertHistory(sameConvoAlixMessages, requiredIDs: Set(expectedApplications.keys))
	}

	func testLastReadTimes() async throws {
		let fixtures = try await fixtures()

		let convoBo = try await fixtures.boClient.conversations.findOrCreateDm(
			with: fixtures.alixClient.inboxID
		)

		Client.register(codec: ReadReceiptCodec())
		let messageID = try await convoBo.send(
			content: ReadReceipt(),
			options: .init(contentType: ReadReceiptCodec().contentType)
		)

		let message = try fixtures.boClient.conversations.findMessage(messageId: messageID)

		let lastReadTimes = try convoBo.getLastReadTimes()

		XCTAssertEqual(lastReadTimes.count, 1)
		XCTAssertEqual(lastReadTimes[fixtures.boClient.inboxID], message?.sentAtNs)
	}
}
