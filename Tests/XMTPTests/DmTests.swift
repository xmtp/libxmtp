//
//  DmTests.swift
//  XMTPiOS
//
//  Created by Naomi Plasterer on 10/23/24.
//

import CryptoKit
import XCTest
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(iOS 16, *)
class DmTests: XCTestCase {
	struct LocalFixtures {
		var alix: PrivateKey!
		var bo: PrivateKey!
		var caro: PrivateKey!
		var alixClient: Client!
		var boClient: Client!
		var caroClient: Client!
	}
	
	func localFixtures() async throws -> LocalFixtures {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let alixClient = try await Client.createV3(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupUpdatedCodec()],
				enableV3: true,
				encryptionKey: key
			)
		)
		let bo = try PrivateKey.generate()
		let boClient = try await Client.createV3(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupUpdatedCodec()],
				enableV3: true,
				encryptionKey: key
			)
		)
		let caro = try PrivateKey.generate()
		let caroClient = try await Client.createV3(
			account: caro,
			options: .init(
				api: .init(env: .local, isSecure: false),
				codecs: [GroupUpdatedCodec()],
				enableV3: true,
				encryptionKey: key
			)
		)
		
		return .init(
			alix: alix,
			bo: bo,
			caro: caro,
			alixClient: alixClient,
			boClient: boClient,
			caroClient: caroClient
		)
	}
	
	func testCanCreateADm() async throws {
		let fixtures = try await localFixtures()

		let convo1 = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
		try await fixtures.alixClient.conversations.sync()
		let sameConvo1 = try await fixtures.alixClient.conversations.findOrCreateDm(with: fixtures.bo.walletAddress)
		XCTAssertEqual(convo1.id, sameConvo1.id)
	}

	func testCanListDmMembers() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
		var members = try await dm.members
		XCTAssertEqual(members.count, 2)

		let peer = try await dm.peerInboxId
		XCTAssertEqual(peer, fixtures.alixClient.inboxID)
	}

	func testCannotStartGroupWithSelf() async throws {
		let fixtures = try await localFixtures()

		await assertThrowsAsyncError(
			try await fixtures.alixClient.conversations.findOrCreateDm(with: fixtures.alix.address)
		)
	}

	func testCannotStartGroupWithNonRegisteredIdentity() async throws {
		let fixtures = try await localFixtures()
		let nonRegistered = try PrivateKey.generate()

		await assertThrowsAsyncError(
			try await fixtures.alixClient.conversations.findOrCreateDm(with: nonRegistered.address)
		)
	}

	func testDmStartsWithAllowedState() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
		_ = try await dm.send(content: "howdy")
		_ = try await dm.send(content: "gm")
		try await dm.sync()

		let isAllowed = try await fixtures.boClient.contacts.isGroupAllowed(groupId: dm.id)
		let dmState = try await fixtures.boClient.contacts.consentList.groupState(groupId: dm.id)
		XCTAssertTrue(isAllowed)
		XCTAssertEqual(dmState, .allowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
	}

	func testCanSendMessageToDm() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
		_ = try await dm.send(content: "howdy")
		let messageId = try await dm.send(content: "gm")
		try await dm.sync()

		let firstMessage = try await dm.messages().first!
		XCTAssertEqual(firstMessage.body, "gm")
		XCTAssertEqual(firstMessage.id, messageId)
		XCTAssertEqual(firstMessage.deliveryStatus, .published)
		let messages = try await dm.messages()
		XCTAssertEqual(messages.count, 3)

		try await fixtures.alixClient.conversations.sync()
		let sameDm = try await fixtures.alixClient.conversations.dms().last!
		try await sameDm.sync()

		let sameMessages = try await sameDm.messages()
		XCTAssertEqual(sameMessages.count, 2)
		XCTAssertEqual(sameMessages.first!.body, "gm")
	}

	func testCanStreamDmMessages() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
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

	func testCanStreamAllDecryptedDmMessages() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)
		try await fixtures.alixClient.conversations.sync()
		
		let expectation1 = XCTestExpectation(description: "got a message")
		expectation1.expectedFulfillmentCount = 2
		
		Task(priority: .userInitiated) {
			for try await _ in await fixtures.alixClient.conversations.streamAllConversationMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await dm.send(content: "hi")
		let caroDm = try await fixtures.caroClient.conversations.findOrCreateDm(with: fixtures.alixClient.address)
		_ = try await caroDm.send(content: "hi")
		
		await fulfillment(of: [expectation1], timeout: 3)
	}

	func testDmConsent() async throws {
		let fixtures = try await localFixtures()

		let dm = try await fixtures.boClient.conversations.findOrCreateDm(with: fixtures.alix.walletAddress)

		let isGroup = try await fixtures.boClient.contacts.isGroupAllowed(groupId: dm.id)
		XCTAssertTrue(isGroup)
		XCTAssertEqual(try dm.consentState(), .allowed)

		try await fixtures.boClient.contacts.denyGroups(groupIds: [dm.id])
		let isDenied = try await fixtures.boClient.contacts.isGroupDenied(groupId: dm.id)
		XCTAssertTrue(isDenied)
		XCTAssertEqual(try dm.consentState(), .denied)

		try await dm.updateConsentState(state: .allowed)
		let isAllowed = try await fixtures.boClient.contacts.isGroupAllowed(groupId: dm.id)
		XCTAssertTrue(isAllowed)
		XCTAssertEqual(try dm.consentState(), .allowed)
	}
}
