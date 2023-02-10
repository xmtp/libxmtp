//
//  ConversationTests.swift
//
//
//  Created by Pat Nakajima on 12/6/22.
//

import CryptoKit
import XCTest
import XMTPTestHelpers
@testable import XMTP

@available(iOS 16, *)
class ConversationTests: XCTestCase {
	var fakeApiClient: FakeApiClient!

	var alice: PrivateKey!
	var aliceClient: Client!

	var bob: PrivateKey!
	var bobClient: Client!

	override func setUp() async throws {
		alice = try PrivateKey.generate()
		bob = try PrivateKey.generate()

		fakeApiClient = FakeApiClient()

		aliceClient = try await Client.create(account: alice, apiClient: fakeApiClient)
		bobClient = try await Client.create(account: bob, apiClient: fakeApiClient)
	}

	func testDoesNotAllowConversationWithSelf() async throws {
		let expectation = expectation(description: "convo with self throws")
		let client = try await Client.create(account: alice)

		do {
			try await client.conversations.newConversation(with: alice.walletAddress)
		} catch {
			expectation.fulfill()
		}

		wait(for: [expectation], timeout: 0.1)
	}

	func testDoesNotIncludeSelfConversationsInList() async throws {
		let convos = try await aliceClient.conversations.list()
		XCTAssert(convos.isEmpty, "setup is wrong")

		let recipient = aliceClient.privateKeyBundleV1.toPublicKeyBundle()
		let invitation = try InvitationV1.createRandom()
		let created = Date()

		let sealedInvitation = try SealedInvitation.createV1(
			sender: aliceClient.keys,
			recipient: SignedPublicKeyBundle(recipient),
			created: created,
			invitation: invitation
		)

		let peerAddress = recipient.walletAddress

		try await aliceClient.publish(envelopes: [
			Envelope(topic: .userInvite(aliceClient.address), timestamp: created, message: try sealedInvitation.serializedData()),
			Envelope(topic: .userInvite(peerAddress), timestamp: created, message: try sealedInvitation.serializedData()),
		])

		let newConvos = try await aliceClient.conversations.list()
		XCTAssert(newConvos.isEmpty, "did not filter out self conversations")
	}

	func testCanStreamConversationsV1() async throws {
		// Overwrite contact as legacy
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		let expectation = expectation(description: "got a conversation")

		Task(priority: .userInitiated) {
			for try await conversation in aliceClient.conversations.stream() {
				if conversation.peerAddress == bob.walletAddress {
					expectation.fulfill()
				}
			}
		}

		guard case let .v1(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not create a v1 convo")
			return
		}

		try await conversation.send(content: "hi")

		// Remove known introduction from contacts to test de-duping
		bobClient.contacts.hasIntroduced.removeAll()

		try await conversation.send(content: "hi again")

		await waitForExpectations(timeout: 5)
	}

	func testCanStreamConversationsV2() async throws {
		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await conversation in bobClient.conversations.stream() {
				expectation1.fulfill()
			}
		}

		guard case let .v2(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation.send(content: "hi")

		guard case let .v2(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation.send(content: "hi again")

		let newWallet = try PrivateKey.generate()
		let newClient = try await Client.create(account: newWallet, apiClient: fakeApiClient)

		guard case let .v2(conversation2) = try await bobClient.conversations.newConversation(with: newWallet.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation2.send(content: "hi from new wallet")

		await waitForExpectations(timeout: 3)
	}

	func testCanUseCachedConversation() async throws {
		guard case .v2 = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not get a v2 convo")
			return
		}

		try await fakeApiClient.assertNoQuery {
			guard case .v2 = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
				XCTFail("Did not get a v2 convo")
				return
			}
		}
	}

	func testCanInitiateV2Conversation() async throws {
		let existingConversations = try await aliceClient.conversations.list()
		XCTAssert(existingConversations.isEmpty, "already had conversations somehow")

		guard case let .v2(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not get a v2 convo")
			return
		}

		let aliceInviteMessage = fakeApiClient.findPublishedEnvelope(.userInvite(alice.walletAddress))
		let bobInviteMessage = fakeApiClient.findPublishedEnvelope(.userInvite(bob.walletAddress))

		XCTAssert(aliceInviteMessage != nil, "no alice invite message")
		XCTAssert(bobInviteMessage != nil, "no bob invite message")

		XCTAssertEqual(conversation.peerAddress, alice.walletAddress)

		let newConversations = try await aliceClient.conversations.list()
		XCTAssertEqual(1, newConversations.count, "already had conversations somehow")
	}

	func testCanFindExistingV1Conversation() async throws {
		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: "hi alice")

		// Get a date that's roughly two weeks ago to test with
		let someTimeAgo = Date().advanced(by: -2_000_000)

		let messageV1 = try MessageV1.encode(
			sender: bobClient.privateKeyBundleV1,
			recipient: aliceClient.privateKeyBundleV1.toPublicKeyBundle(),
			message: try encodedContent.serializedData(),
			timestamp: someTimeAgo
		)

		// Overwrite contact as legacy
		try await bobClient.publishUserContact(legacy: true)
		try await aliceClient.publishUserContact(legacy: true)

		try await bobClient.publish(envelopes: [
			Envelope(topic: .userIntro(bob.walletAddress), timestamp: someTimeAgo, message: try Message(v1: messageV1).serializedData()),
			Envelope(topic: .userIntro(alice.walletAddress), timestamp: someTimeAgo, message: try Message(v1: messageV1).serializedData()),
			Envelope(topic: .directMessageV1(bob.walletAddress, alice.walletAddress), timestamp: someTimeAgo, message: try Message(v1: messageV1).serializedData()),
		])

		guard case let .v1(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a convo with bob")
			return
		}

		XCTAssertEqual(conversation.peerAddress, bob.walletAddress)
		XCTAssertEqual(Int(conversation.sentAt.timeIntervalSince1970), Int(someTimeAgo.timeIntervalSince1970))

		let existingMessages = fakeApiClient.published.count

		guard case let .v1(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not have a convo with alice")
			return
		}

		XCTAssertEqual(existingMessages, fakeApiClient.published.count, "published more messages when we shouldn't have")
		XCTAssertEqual(conversation.peerAddress, alice.walletAddress)
		XCTAssertEqual(Int(conversation.sentAt.timeIntervalSince1970), Int(someTimeAgo.timeIntervalSince1970))
	}

	func testCanFindExistingV2Conversation() async throws {
		guard case let .v2(existingConversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress, context: .init(conversationID: "http://example.com/2")) else {
			XCTFail("Did not create existing conversation with alice")
			return
		}

		try await fakeApiClient.assertNoPublish {
			guard case let .v2(conversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress, context: .init(conversationID: "http://example.com/2")) else {
				XCTFail("Did not get conversation with bob")
				return
			}

			XCTAssertEqual(conversation.topic, existingConversation.topic, "made new conversation instead of using existing one")
		}
	}

	func publishLegacyContact(client: Client) async throws {
		var contactBundle = ContactBundle()
		contactBundle.v1.keyBundle = client.privateKeyBundleV1.toPublicKeyBundle()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(client.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()

		try await client.publish(envelopes: [envelope])
	}

	func testStreamingMessagesFromV1Conversation() async throws {
		// Overwrite contact as legacy
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a convo with bob")
			return
		}

		let expectation = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in conversation.streamMessages() {
				expectation.fulfill()
			}
		}

		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: "hi alice")

		let date = Date().advanced(by: -1_000_000)

		// Stream a message
		fakeApiClient.send(
			envelope: Envelope(
				topic: conversation.topic,
				timestamp: Date(),
				message: try Message(
					v1: MessageV1.encode(
						sender: bobClient.privateKeyBundleV1,
						recipient: aliceClient.privateKeyBundleV1.toPublicKeyBundle(),
						message: try encodedContent.serializedData(),
						timestamp: date
					)
				).serializedData()
			)
		)

		await waitForExpectations(timeout: 3)
	}

	func testStreamingMessagesFromV2Conversations() async throws {
		guard case let .v2(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not get a v2 convo")
			return
		}

		let expectation = expectation(description: "got a message")

		Task(priority: .userInitiated) {
			for try await _ in conversation.streamMessages() {
				expectation.fulfill()
			}
		}

		let encoder = TextCodec()
		let encodedContent = try encoder.encode(content: "hi alice")

		// Stream a message
		fakeApiClient.send(
			envelope: Envelope(
				topic: conversation.topic,
				timestamp: Date(),
				message: try Message(
					v2: try await MessageV2.encode(
						client: bobClient,
						content: encodedContent,
						topic: conversation.topic,
						keyMaterial: conversation.keyMaterial
					)
				).serializedData()
			)
		)

		await waitForExpectations(timeout: 3)
	}

	func testCanLoadV1Messages() async throws {
		// Overwrite contact as legacy so we can get v1
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice")
		try await bobConversation.send(content: "hey alice again")

		let messages = try await aliceConversation.messages()

		XCTAssertEqual(2, messages.count)
		XCTAssertEqual("hey alice", messages[1].body)
		XCTAssertEqual(bob.address, messages[1].senderAddress)
	}

	func testCanLoadV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice")
		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice", messages[0].body)
		XCTAssertEqual(bob.address, messages[0].senderAddress)
	}

	func testVerifiesV2MessageSignature() async throws {
		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		let codec = TextCodec()
		let originalContent = try codec.encode(content: "hello")
		let tamperedContent = try codec.encode(content: "this is a fake")

		let originalPayload = try originalContent.serializedData()
		let tamperedPayload = try tamperedContent.serializedData()

		let date = Date()
		let header = MessageHeaderV2(topic: aliceConversation.topic, created: date)
		let headerBytes = try header.serializedData()

		let digest = SHA256.hash(data: headerBytes + tamperedPayload)
		let preKey = aliceClient.keys.preKeys[0]
		let signature = try await preKey.sign(Data(digest))

		let bundle = aliceClient.privateKeyBundleV1.toV2().getPublicKeyBundle()

		let signedContent = SignedContent(payload: originalPayload, sender: bundle, signature: signature)
		let signedBytes = try signedContent.serializedData()

		let ciphertext = try Crypto.encrypt(aliceConversation.keyMaterial, signedBytes, additionalData: headerBytes)

		let tamperedMessage = MessageV2(
			headerBytes: headerBytes,
			ciphertext: ciphertext
		)

		try await aliceClient.publish(envelopes: [
			Envelope(topic: aliceConversation.topic, timestamp: Date(), message: try Message(v2: tamperedMessage).serializedData()),
		])

		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		let messages = try await bobConversation.messages()
		XCTAssertEqual(0, messages.count, "did not filter out tampered message")
	}

	func testCanPaginateV1Messages() async throws {
		// Overwrite contact as legacy so we can get v1
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice 1", sentAt: Date().addingTimeInterval(-10))
		try await bobConversation.send(content: "hey alice 2", sentAt: Date().addingTimeInterval(-5))
		try await bobConversation.send(content: "hey alice 3", sentAt: Date())

		let messages = try await aliceConversation.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice 3", messages[0].body)

		let messages2 = try await aliceConversation.messages(limit: 1, before: messages[0].sent)
		XCTAssertEqual(1, messages2.count)
		XCTAssertEqual("hey alice 2", messages2[0].body)
	}

	func testCanPaginateV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: "hey alice 1", sentAt: Date().addingTimeInterval(-10))
		try await bobConversation.send(content: "hey alice 2", sentAt: Date().addingTimeInterval(-5))
		try await bobConversation.send(content: "hey alice 3", sentAt: Date())

		let messages = try await aliceConversation.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hey alice 3", messages[0].body)

		let messages2 = try await aliceConversation.messages(limit: 1, before: messages[0].sent)
		XCTAssertEqual(1, messages2.count)
		XCTAssertEqual("hey alice 2", messages2[0].body)
	}

	func testImportV1ConversationFromJS() async throws {
		let jsExportJSONData = Data("""
		{
				"version": "v1",
				"peerAddress": "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14",
				"createdAt": "2022-09-20T09:32:50.329Z"
		}
		""".utf8)

		let conversation = try aliceClient.importConversation(from: jsExportJSONData)

		XCTAssertEqual(conversation?.peerAddress, "0x5DAc8E2B64b8523C11AF3e5A2E087c2EA9003f14")
	}

	func testImportV2ConversationFromJS() async throws {
		let jsExportJSONData = Data("""
		{"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z","context":{"conversationId":"pat/messageid","metadata":{}}}
		""".utf8)

		let conversation = try aliceClient.importConversation(from: jsExportJSONData)
		XCTAssertEqual(conversation?.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
	}

	func testImportV2ConversationWithNoContextFromJS() async throws {
		let jsExportJSONData = Data("""
		{"version":"v2","topic":"/xmtp/0/m-2SkdN5Qa0ZmiFI5t3RFbfwIS-OLv5jusqndeenTLvNg/proto","keyMaterial":"ATA1L0O2aTxHmskmlGKCudqfGqwA1H+bad3W/GpGOr8=","peerAddress":"0x436D906d1339fC4E951769b1699051f020373D04","createdAt":"2023-01-26T22:58:45.068Z"}
		""".utf8)

		guard case let .v2(conversation) = try aliceClient.importConversation(from: jsExportJSONData) else {
			XCTFail("did not get a v2 conversation")
			return
		}

		XCTAssertEqual(conversation.peerAddress, "0x436D906d1339fC4E951769b1699051f020373D04")
	}

	func testV1ConversationCodable() async throws {
		// Overwrite contact as legacy
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a v1 convo with bob")
			return
		}
		try await conversation.send(content: "hi")
		let envelope = fakeApiClient.published.first(where: { $0.contentTopic.hasPrefix("/xmtp/0/dm-") })!

		let container = Conversation.v1(conversation).encodedContainer

		try await fakeApiClient.assertNoQuery {
			let decodedConversation = container.decode(with: aliceClient)
			let decodedMessage = try decodedConversation.decode(envelope)
			XCTAssertEqual(decodedMessage.body, "hi")
		}
	}

	func testV2ConversationCodable() async throws {
		guard case let .v2(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a v2 convo with bob")
			return
		}
		try await conversation.send(content: "hi")
		let envelope = fakeApiClient.published.first(where: { $0.contentTopic.hasPrefix("/xmtp/0/m-") })!

		let container = Conversation.v2(conversation).encodedContainer

		try await fakeApiClient.assertNoQuery {
			let decodedConversation = container.decode(with: aliceClient)
			let decodedMessage = try decodedConversation.decode(envelope)
			XCTAssertEqual(decodedMessage.body, "hi")
		}
	}

	func testDecodeSingleV1Message() async throws {
		// Overwrite contact as legacy
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a convo with bob")
			return
		}

		try await conversation.send(content: "hi")

		let message = fakeApiClient.published.first(where: { $0.contentTopic.hasPrefix("/xmtp/0/dm-") })!

		let decodedMessage = try conversation.decode(envelope: message)
		XCTAssertEqual("hi", decodedMessage.body)

		let decodedMessage2 = try Conversation.v1(conversation).decode(message)
		XCTAssertEqual("hi", decodedMessage2.body)
	}

	func testDecodeSingleV2Message() async throws {
		guard case let .v2(conversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not have a convo with bob")
			return
		}

		try await conversation.send(content: "hi")

		let message = fakeApiClient.published.first(where: { $0.contentTopic.hasPrefix("/xmtp/0/m-") })!

		let decodedMessage = try conversation.decode(envelope: message)
		XCTAssertEqual("hi", decodedMessage.body)

		let decodedMessage2 = try Conversation.v2(conversation).decode(message)
		XCTAssertEqual("hi", decodedMessage2.body)
	}

	func testCanSendGzipCompressedV1Messages() async throws {
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		try await bobConversation.send(content: Array(repeating: "A", count: 1000).joined(), options: .init(compression: .gzip))

		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual(Array(repeating: "A", count: 1000).joined(), try messages[0].content())
	}

	func testCanSendDeflateCompressedV1Messages() async throws {
		try await publishLegacyContact(client: bobClient)
		try await publishLegacyContact(client: aliceClient)

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address) else {
			XCTFail("did not get a v1 conversation for alice")
			return
		}

		try await bobConversation.send(content: Array(repeating: "A", count: 1000).joined(), options: .init(compression: .deflate))

		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual(Array(repeating: "A", count: 1000).joined(), try messages[0].content())
	}

	func testCanSendGzipCompressedV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: Array(repeating: "A", count: 1000).joined(), options: .init(compression: .gzip))
		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual(Array(repeating: "A", count: 1000).joined(), messages[0].body)
		XCTAssertEqual(bob.address, messages[0].senderAddress)
	}

	func testCanSendDeflateCompressedV2Messages() async throws {
		guard case let .v2(bobConversation) = try await bobClient.conversations.newConversation(with: alice.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		guard case let .v2(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.address, context: InvitationV1.Context(conversationID: "hi")) else {
			XCTFail("did not get a v2 conversation for alice")
			return
		}

		try await bobConversation.send(content: Array(repeating: "A", count: 1000).joined(), options: .init(compression: .deflate))
		let messages = try await aliceConversation.messages()

		XCTAssertEqual(1, messages.count)
		XCTAssertEqual(Array(repeating: "A", count: 1000).joined(), messages[0].body)
		XCTAssertEqual(bob.address, messages[0].senderAddress)
	}
}
