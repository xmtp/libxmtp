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
	override func setUp() {
		super.setUp()
		setupLocalEnv()
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
				dbDirectory: dbDir1,
				// useDefaultHistorySyncUrl: false
			),
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID],
		)
		try await group.updateConsentState(state: .denied)
		XCTAssertEqual(try group.consentState(), .denied)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
				//                useDefaultHistorySyncUrl: false
			),
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await alixClient2.preferences.sync()
		try await alixClient.conversations.syncAllConversations()
		sleep(2)
		try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let dm2 = try await alixClient2.conversations.findConversation(
			conversationId: group.id,
		) {
			XCTAssertEqual(try dm2.consentState(), .denied)

			try await alixClient2.preferences.setConsentState(
				entries: [
					ConsentRecord(
						value: dm2.id,
						entryType: .conversation_id,
						consentType: .allowed,
					),
				],
			)
			let convoState = try await alixClient2.preferences
				.conversationState(
					conversationId: dm2.id,
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
			),
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID],
		)
		let messageCount = try await group.messages().count
		XCTAssertEqual(messageCount, 1)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
			),
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
			groupId: group.id,
		) {
			let messageCount2 = try await group2.messages().count
			XCTAssertEqual(messageCount2, 2)
		} else {
			XCTFail("Group not found")
		}
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
				deviceSyncEnabled: true,
			),
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
				deviceSyncEnabled: true,
			),
		)

		try await alixGroup.send(content: "Hello")
		try await alixClient.conversations.syncAllConversations()
		try await alixClient2.conversations.syncAllConversations()
		let alixGroup2Result = try await alixClient2.conversations.findGroup(
			groupId: alixGroup.id,
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
			with: fixtures.caroClient.inboxID,
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
				dbDirectory: dbDir1,
			),
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
				dbDirectory: dbDir2,
			),
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
				dbDirectory: dbDir1,
				useDefaultHistorySyncUrl: false,
			),
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID],
		)
		try await group.updateConsentState(state: .denied)
		XCTAssertEqual(try group.consentState(), .denied)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
				useDefaultHistorySyncUrl: false,
			),
		)

		let state = try await alixClient2.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 2)

		try await alixClient2.preferences.sync()
		try await alixClient.conversations.syncAllConversations()
		sleep(2)
		try await alixClient2.conversations.syncAllConversations()
		sleep(2)

		if let dm2 = try await alixClient2.conversations.findConversation(
			conversationId: group.id,
		) {
			XCTAssertEqual(try dm2.consentState(), .denied)

			try await alixClient2.preferences.setConsentState(
				entries: [
					ConsentRecord(
						value: dm2.id,
						entryType: .conversation_id,
						consentType: .allowed,
					),
				],
			)
			let convoState = try await alixClient2.preferences
				.conversationState(
					conversationId: dm2.id,
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
				dbDirectory: dbDir1,
				useDefaultHistorySyncUrl: false,
			),
		)

		let group = try await alixClient.conversations.newGroup(
			with: [fixtures.boClient.inboxID],
		)
		let messageCount = try await group.messages().count
		XCTAssertEqual(messageCount, 1)

		let alixClient2 = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: XMTPEnvironment.local.isSecure),
				dbEncryptionKey: key,
				dbDirectory: dbDir2,
			),
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
			groupId: group.id,
		) {
			let messageCount2 = try await group2.messages().count
			XCTAssertEqual(messageCount2, 2)
		} else {
			XCTFail("Could not find group")
		}
	}
}
