//
//  ConversationsTests.swift
//
//
//  Created by Pat on 2/16/23.
//

import Foundation
import XCTest
@testable import XMTP

@available(iOS 15, *)
class ConversationsTests: XCTestCase {
	func testCanGetConversationFromIntroEnvelope() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		let created = Date()
		let newWallet = try PrivateKey.generate()
		let newClient = try await Client.create(account: newWallet, apiClient: fixtures.fakeApiClient)

		let message = try MessageV1.encode(
			sender: newClient.privateKeyBundleV1,
			recipient: fixtures.aliceClient.v1keys.toPublicKeyBundle(),
			message: try TextCodec().encode(content: "hello").serializedData(),
			timestamp: created
		)

		let envelope = Envelope(topic: .userIntro(client.address), timestamp: created, message: try Message(v1: message).serializedData())

		let conversation = try client.conversations.fromIntro(envelope: envelope)
		XCTAssertEqual(conversation.peerAddress, newWallet.address)
		XCTAssertEqual(conversation.createdAt.description, created.description)
	}

	func testCanGetConversationFromInviteEnvelope() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		let created = Date()
		let newWallet = try PrivateKey.generate()
		let newClient = try await Client.create(account: newWallet, apiClient: fixtures.fakeApiClient)

		let invitation = try InvitationV1.createRandom(context: nil)
		let sealed = try SealedInvitation.createV1(
			sender: newClient.keys,
			recipient: client.keys.getPublicKeyBundle(),
			created: created,
			invitation: invitation
		)

		let peerAddress = fixtures.alice.walletAddress
		let envelope = Envelope(topic: .userInvite(peerAddress), timestamp: created, message: try sealed.serializedData())

		let conversation = try client.conversations.fromInvite(envelope: envelope)
		XCTAssertEqual(conversation.peerAddress, newWallet.address)
		XCTAssertEqual(conversation.createdAt.description, created.description)
	}

	func testStreamAllMessagesGetsMessageFromKnownConversation() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		let bobConversation = try await fixtures.bobClient.conversations.newConversation(with: client.address)

		let expectation1 = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in try await client.conversations.streamAllMessages() {
				expectation1.fulfill()
			}
		}

		_ = try await bobConversation.send(text: "hi")

		await waitForExpectations(timeout: 3)
	}

	@available(iOS 16.0, *)
	func testStreamAllMessagesWorksWithInvites() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		let expectation1 = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in try await client.conversations.streamAllMessages() {
				expectation1.fulfill()
			}
		}

		let bobConversation = try await fixtures.bobClient.conversations.newConversation(with: client.address)
		try await Task.sleep(for: .milliseconds(100))
		_ = try await bobConversation.send(text: "hi")

		await waitForExpectations(timeout: 3)
	}

	@available(iOS 16.0, *)
	func testStreamAllMessagesWorksWithIntros() async throws {
		let fixtures = await fixtures()
		let client = fixtures.aliceClient!

		// Overwrite contact as legacy
		try await fixtures.publishLegacyContact(client: fixtures.bobClient)
		try await fixtures.publishLegacyContact(client: fixtures.aliceClient)

		let expectation1 = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in try await client.conversations.streamAllMessages() {
				expectation1.fulfill()
			}
		}

		let bobConversation = try await fixtures.bobClient.conversations.newConversation(with: client.address)
		XCTAssertEqual(bobConversation.version, .v1)

		try await Task.sleep(for: .milliseconds(100))
		_ = try await bobConversation.send(text: "hi")

		await waitForExpectations(timeout: 3)
	}
}
