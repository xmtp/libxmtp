//
//  PaginationTests.swift
//
//
//  Created by Michael Xu on 05/16/23.
//

import Foundation

import XCTest
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(iOS 15, *)
class PaginationTests: XCTestCase {

	func newClientHelper(account: PrivateKey) async throws -> Client {
		let client = try await Client.create(account: account, options: ClientOptions(api: .init(env: .local, isSecure: false)))
		return client
	}

	func testLongConvo() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let aliceClient = try await newClientHelper(account: alice)
		let bobClient = try await newClientHelper(account: bob)

		let canAliceMessageBob = try await aliceClient.canMessage(bobClient.address)
		XCTAssert(canAliceMessageBob)

		// Start a conversation with alice

		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice 1", sentAt: Date().addingTimeInterval(-1000))
		try await bobConversation.send(content: "hey alice 2", sentAt: Date().addingTimeInterval(-500))
		try await bobConversation.send(content: "hey alice 3", sentAt: Date())

		let messages = try await aliceConversation.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice 3", messages[0].body)

		let messages2 = try await aliceConversation.messages(limit: 1, before: messages[0].sent)
		XCTAssertEqual(1, messages2.count)
		XCTAssertEqual("hey alice 2", messages2[0].body)

		// Send many many more messages, such that it forces cursor saving and pagination
		for i in 4..<101 {
			try await bobConversation.send(content: "hey alice \(i)", sentAt: Date())
		}
		// Grab the messages 50 at a time
		let messages3 = try await aliceConversation.messages(limit: 50)
		XCTAssertEqual(50, messages3.count)
		XCTAssertEqual("hey alice 100", messages3[0].body)
		XCTAssertEqual("hey alice 51", messages3[49].body)

		let messages4 = try await aliceConversation.messages(limit: 100, before: messages3[49].sent)
		XCTAssertEqual(50, messages4.count)
		XCTAssertEqual("hey alice 50", messages4[0].body)
		XCTAssertEqual("hey alice 1", messages4[49].body)
	}

	func testCanStreamConversationsV2() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		// Need to upload Alice's contact bundle
		let _ = try await newClientHelper(account: alice)
		let bobClient = try await newClientHelper(account: bob)
		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await _ in try await bobClient.conversations.stream() {
				print("Got one conversation")
				expectation1.fulfill()
			}
		}

		guard case let .v2(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation.send(content: "hi")

		let newWallet = try PrivateKey.generate()
		// Need to upload contact bundle
		let _ = try await newClientHelper(account: newWallet)
		guard case let .v2(conversation2) = try await bobClient.conversations.newConversation(with: newWallet.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation2.send(content: "hi from new wallet")

		await waitForExpectations(timeout: 5)

		// Test that we can stream a few more messages
		let expectation2 = expectation(description: "got follow-up messages")
		expectation2.expectedFulfillmentCount = 5
		Task(priority: .userInitiated) {
			for try await message in conversation.streamMessages() {
				print("Got message: \(message)")
				expectation2.fulfill()
			}
		}

		// Slowly send out messages
		Task(priority: .userInitiated) {
			try! await conversation.send(content: "hi")
			try! await conversation.send(content: "hi again")
			try! await conversation.send(content: "hi again again")
			try! await conversation.send(content: "hi again again again")
			try! await conversation.send(content: "hi again again again again")
		}

		await waitForExpectations(timeout: 5)
	}
}
