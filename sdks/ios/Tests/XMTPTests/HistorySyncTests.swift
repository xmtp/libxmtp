//
//  HistorySyncTests.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 12/19/24.
//

import Foundation
import XCTest
@testable import XMTPiOS

@available(iOS 15, *)
class HistorySyncTests: XCTestCase {
	private let syncTimeoutNs: UInt64 = 5_000_000_000
	private let syncPollIntervalNs: UInt64 = 500_000_000

	override func setUp() {
		super.setUp()
		setupLocalEnv()
	}

	private func waitForSyncCondition(
		description: String,
		timeoutNs: UInt64? = nil,
		pollIntervalNs: UInt64? = nil,
		condition: () async throws -> Bool
	) async throws {
		let timeout = timeoutNs ?? syncTimeoutNs
		let pollInterval = pollIntervalNs ?? syncPollIntervalNs
		let start = DispatchTime.now().uptimeNanoseconds

		while DispatchTime.now().uptimeNanoseconds - start < timeout {
			if try await condition() {
				return
			}
			try await Task.sleep(nanoseconds: pollInterval)
		}

		XCTFail("Timed out waiting for \(description)")
	}

	func testSyncConsent() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1
			)
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await group.updateConsentState(state: .denied)
		XCTAssertEqual(try group.consentState(), .denied)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2
			)
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await alixClient2.preferences.sync()
		_ = try await alixClient.conversations.syncAllConversations()
		sleep(2)
		_ = try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let dm2 = try await alixClient2.conversations.findConversation(
			conversationId: group.id
		) {
			XCTAssertEqual(try dm2.consentState(), .denied)

			try await alixClient2.preferences.setConsentState(
				entries: [
					ConsentRecord(
						value: dm2.id,
						entryType: .conversation_id,
						consentType: .allowed
					),
				]
			)
			let convoState = try await alixClient2.preferences
				.conversationState(
					conversationId: dm2.id
				)
			XCTAssertEqual(convoState, .allowed)
			XCTAssertEqual(try dm2.consentState(), .allowed)
		}
	}

	func testSyncMessages() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1,
				deviceSyncEnabled: true // this is default, but marking for clarity
			)
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)

		// To verify device sync works, we need to send a message before client 2 is added to the group
		let msg_id = try await group.send(content: "hi")
		let messageCount = try await group.messages().count
		XCTAssertEqual(messageCount, 2)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2
			)
		)
		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await alixClient2.sendSyncRequest()
		sleep(1)
		var lastClient1MessageCount = 0
		var lastClient2MessageCount = 0
		var seenMessageOnClient2 = false

		try await waitForSyncCondition(
			description: "synced message parity between installations",
			condition: {
			_ = try await alixClient.syncAllDeviceSyncGroups()
			_ = try await alixClient2.syncAllDeviceSyncGroups()
			_ = try await alixClient.conversations.syncAllConversations()
			_ = try await alixClient2.conversations.syncAllConversations()

			lastClient1MessageCount = try await group.messages().count

			if let group2 = try alixClient2.conversations.findGroup(groupId: group.id) {
				try await group2.sync()
				let messages = try await group2.messages()
				lastClient2MessageCount = messages.count
				seenMessageOnClient2 = messages.contains(where: { $0.id == msg_id })
			}

			return seenMessageOnClient2
				&& lastClient1MessageCount == lastClient2MessageCount
			}
		)

		if !seenMessageOnClient2 || lastClient1MessageCount != lastClient2MessageCount {
			XCTFail(
				"Message parity not reached; client1Count=\(lastClient1MessageCount) client2Count=\(lastClient2MessageCount) seenMessageOnClient2=\(seenMessageOnClient2)"
			)
		}
	}

	func testSyncDeviceArchive() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()

		// Alix creates a group with Bo and sends a message before Alix2 exists.
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1,
				deviceSyncEnabled: true
			)
		)
		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		let msgFromAlix = try await group.send(content: "hello from alix")

		// Create Alix second installation.
		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
				deviceSyncEnabled: true
			)
		)

		sleep(1)

		// Alix syncs and uploads a sync archive with a known pin.
		_ = try await alixClient.syncAllDeviceSyncGroups()
		try await alixClient.sendSyncArchive(pin: "123")

		// Give the archive upload a brief moment to become available.
		sleep(1)

		// Bo sends a new message after Alix2 was created.
		_ = try await fixtures.boClient.conversations.syncAllConversations()
		let maybeBoGroup = try fixtures.boClient.conversations.findGroup(
			groupId: group.id
		)
		let boGroup = try XCTUnwrap(maybeBoGroup)
		_ = try await boGroup.send(content: "hello from bo")

		// Sync both Alix clients.
		_ = try await alixClient.conversations.syncAllConversations()
		_ = try await alixClient2.conversations.syncAllConversations()

		// Before importing archive, Alix2 should only have post-installation visibility.
		let maybeGroup2Before = try alixClient2.conversations.findGroup(
			groupId: group.id
		)
		let group2Before = try XCTUnwrap(maybeGroup2Before)
		let messagesBefore = try await group2Before.messages()
		XCTAssertEqual(
			messagesBefore.count,
			2,
			"Expected two messages before archive import"
		)

		// Pull sync-group updates and verify the archive pin is visible.
		sleep(2)
		_ = try await alixClient.syncAllDeviceSyncGroups()
		sleep(2)
		_ = try await alixClient2.syncAllDeviceSyncGroups()
		let availableArchives = try alixClient2.listAvailableArchives(daysCutoff: 7)
		XCTAssertTrue(
			availableArchives.contains(where: { $0.pin == "123" }),
			"Expected archive pin 123 to be available before import"
		)

		// Import archive and verify Alix's original message becomes visible.
		try await alixClient2.processSyncArchive(archivePin: "123")
		_ = try await alixClient2.conversations.syncAllConversations()

		let maybeGroup2After = try alixClient2.conversations.findGroup(
			groupId: group.id
		)
		let group2After = try XCTUnwrap(maybeGroup2After)
		let messagesAfter = try await group2After.messages()
		XCTAssertEqual(
			messagesAfter.count,
			3,
			"Expected three messages after archive import"
		)
		XCTAssertTrue(
			messagesAfter.contains(where: { $0.id == msgFromAlix }),
			"Expected original Alix message to be visible after archive import"
		)
	}

	func testStreamConsent() async throws {
		throw XCTSkip("Skipped: Test is flaky")
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1,
				deviceSyncEnabled: true
			)
		)

		let alixGroup = try await alixClient.conversations.newGroup(with: [
			fixtures.boClient.inboxID,
		])

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
				deviceSyncEnabled: true
			)
		)

		try await alixGroup.send(content: "Hello")
		try await alixClient.conversations.syncAllConversations()
		try await alixClient2.conversations.syncAllConversations()
		let alixGroup2Result = try await alixClient2.conversations.findGroup(
			groupId: alixGroup.id
		)
		let alixGroup2 = try XCTUnwrap(alixGroup2Result)

		var consentList = [ConsentRecord]()
		let expectation = XCTestExpectation(description: "Stream Consent")
		expectation.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await entry in await alixClient.preferences.streamConsent() {
				expectation.fulfill()
				consentList.append(entry)
			}
		}
		sleep(5)
		try await alixGroup2.updateConsentState(state: .denied)

		let dm = try await alixClient2.conversations.newConversation(
			with: fixtures.caroClient.inboxID
		)
		try await dm.updateConsentState(state: .denied)

		sleep(5)
		try await alixClient.conversations.syncAllConversations()
		try await alixClient2.conversations.syncAllConversations()

		await fulfillment(of: [expectation], timeout: 10)
		XCTAssertEqual(try alixGroup.consentState(), .denied)
	}

	func testStreamPrivatePreferences() async throws {
		throw XCTSkip("Skipped: Test is flaky")
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1
			)
		)

		let expectation = XCTestExpectation(description: "Stream Preferences")
		expectation.expectedFulfillmentCount = 1

		Task(priority: .userInitiated) {
			for try await _ in await alixClient.preferences
				.streamPreferenceUpdates()
			{
				expectation.fulfill()
			}
		}

		sleep(2)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2
			)
		)

		try await alixClient2.conversations.syncAllConversations()
		try await alixClient.conversations.syncAllConversations()

		await fulfillment(of: [expectation], timeout: 10)
	}

	func testDisablingHistoryTransferStillSyncsLocalState() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1
			)
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		try await group.updateConsentState(state: .denied)
		XCTAssertEqual(try group.consentState(), .denied)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2
			)
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await alixClient2.preferences.sync()
		try await alixClient.conversations.syncAllConversations()
		sleep(2)
		try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let dm2 = try await alixClient2.conversations.findConversation(
			conversationId: group.id
		) {
			XCTAssertEqual(try dm2.consentState(), .denied)

			try await alixClient2.preferences.setConsentState(
				entries: [
					ConsentRecord(
						value: dm2.id,
						entryType: .conversation_id,
						consentType: .allowed
					),
				]
			)
			let convoState = try await alixClient2.preferences
				.conversationState(
					conversationId: dm2.id
				)
			XCTAssertEqual(convoState, .allowed)
			XCTAssertEqual(try dm2.consentState(), .allowed)
		}
	}

	func testDisablingHistoryTransferDoesNotTransfer() async throws {
		let fixtures = try await fixtures()

		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let dbDir1 = randomDbDirectory()
		let dbDir2 = randomDbDirectory()
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir1
			)
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID]
		)
		let messageCount = try await group.messages().count
		XCTAssertEqual(messageCount, 1)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2
			)
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		// If we move this line before alixClient2 create, we fail with the group
		// not being found. history sync seems to get messages, but maybe
		// not groups?
		try await group.send(content: "hi")

		try await alixClient.conversations.syncAllConversations()
		sleep(2)
		try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let group2 = try await alixClient2.conversations.findGroup(
			groupId: group.id
		) {
			let messageCount2 = try await group2.messages().count
			XCTAssertEqual(messageCount2, 2)
		} else {
			XCTFail("Could not find group")
		}
	}
}
