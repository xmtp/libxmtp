//
//  IntegrationTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import web3
import XCTest
import LibXMTP
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(macOS 13.0, *)
@available(iOS 16, *)
final class IntegrationTests: XCTestCase {
	func testSaveKey() async throws {
		let alice = try PrivateKey.generate()
		let identity = try PrivateKey.generate()

		let authorized = try await alice.createIdentity(identity)

		let authToken = try await authorized.createAuthToken()

		let rustClient = try await LibXMTP.createV2Client(host: XMTPEnvironment.local.url, isSecure: false)
		let api = try GRPCApiClient(environment: .local, secure: false, rustClient: rustClient)
		api.setAuthToken(authToken)

		let encryptedBundle = try await authorized.toBundle.encrypted(with: alice)

		var envelope = Envelope()
		envelope.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch) * 1_000_000
		envelope.message = try encryptedBundle.serializedData()

		try await api.publish(envelopes: [envelope])

		try await Task.sleep(nanoseconds: 2_000_000_000)

		let result = try await api.query(topic: .userPrivateStoreKeyBundle(authorized.address))
		XCTAssert(result.envelopes.count == 1)
	}

	func testPublishingAndFetchingContactBundles() async throws {
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))

		let aliceWallet = try PrivateKey.generate()
		let alice = try await Client.create(account: aliceWallet, options: opts)
		try await delayToPropagate()
		let contact = try await alice.getUserContact(peerAddress: alice.address)

		XCTAssertEqual(contact!.v2.keyBundle.identityKey.secp256K1Uncompressed, alice.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed)
		XCTAssert(contact!.v2.keyBundle.identityKey.hasSignature == true, "no signature")
		XCTAssert(contact!.v2.keyBundle.preKey.hasSignature == true, "pre key not signed")

		let aliceAgain = try await Client.create(account: aliceWallet, options: opts)
		try await delayToPropagate()
		let contactAgain = try await alice.getUserContact(peerAddress: alice.address)
		XCTAssertEqual(contactAgain!, contact!, "contact bundle should not have changed")
	}

	func testCanReceiveV1MessagesFromJS() async throws {
		throw XCTSkip("integration only (requires local node)")

		let wallet = try FakeWallet.generate()
		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let client = try await Client.create(account: wallet, options: options)

		let convo = ConversationV1(client: client, peerAddress: "0xf4BF19Ed562651837bc11ff975472ABd239D35B5", sentAt: Date())
		try await convo.send(content: "hello from swift")
		try await Task.sleep(for: .seconds(1))

		let messages = try await convo.messages()
		XCTAssertEqual(2, messages.count)

		XCTAssertEqual("HI \(wallet.address)", messages[0].body)
	}

	func testCanReceiveV2MessagesFromJS() async throws {
		throw XCTSkip("integration only (requires local node)")

		let wallet = try PrivateKey.generate()
		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let client = try await Client.create(account: wallet, options: options)

		try await client.publishUserContact()

		guard case let .v2(convo) = try? await client.conversations.newConversation(with: "0xf4BF19Ed562651837bc11ff975472ABd239D35B5", context: InvitationV1.Context(conversationID: "https://example.com/4")) else {
			XCTFail("did not get v2 convo")
			return
		}

		try await convo.send(content: "hello from swift")
		try await Task.sleep(for: .seconds(1))

		let messages = try await convo.messages()
		XCTAssertEqual(2, messages.count)
		XCTAssertEqual("HI \(wallet.address)", messages[0].body)
	}

	func testEndToEndConversation() async throws {
		let opt = ClientOptions(api: .init(env: .local, isSecure: false))
        let alice = try await Client.create(account: try PrivateKey.generate(), options: opt)
        let bob = try await Client.create(account: try PrivateKey.generate(), options: opt)

        let aliceConvo = try await alice.conversations.newConversation(with: bob.address)
        _ = try await aliceConvo.send(text: "Hello Bob")
        try await delayToPropagate()

        let bobConvos = try await bob.conversations.list()
        let bobConvo = bobConvos[0]
        let bobSees = try await bobConvo.messages()
        XCTAssertEqual("Hello Bob", bobSees[0].body)

        try await bobConvo.send(text: "Oh, hello Alice")
        try await delayToPropagate()

        let aliceSees = try await aliceConvo.messages()
        XCTAssertEqual("Hello Bob", aliceSees[1].body)
        XCTAssertEqual("Oh, hello Alice", aliceSees[0].body)
	}

	func testUsingSavedCredentialsAndKeyMaterial() async throws {
		let opt = ClientOptions(api: .init(env: .local, isSecure: false))
		let alice = try await Client.create(account: try PrivateKey.generate(), options: opt)
		let bob = try await Client.create(account: try PrivateKey.generate(), options: opt)

		// Alice starts a conversation with Bob
		let aliceConvo = try await alice.conversations.newConversation(
				with: bob.address,
				context: InvitationV1.Context.with {
					$0.conversationID = "example.com/alice-bob-1"
					$0.metadata["title"] = "Chatting Using Saved Credentials"
				})
		_ = try await aliceConvo.send(text: "Hello Bob")
		try await delayToPropagate()

		// Alice stores her credentials and conversations to her device
		let keyBundle = try alice.privateKeyBundle.serializedData();
		let topicData = try aliceConvo.toTopicData().serializedData();

		// Meanwhile, Bob sends a reply.
		let bobConvos = try await bob.conversations.list()
		let bobConvo = bobConvos[0]
		try await bobConvo.send(text: "Oh, hello Alice")
		try await delayToPropagate()

		// When Alice's device wakes up, it uses her saved credentials
		let alice2 = try await Client.from(
			bundle: PrivateKeyBundle(serializedData: keyBundle),
			options: opt
		)
		// And it uses the saved topic data for the conversation
		let aliceConvo2 = await alice2.conversations.importTopicData(
				data: try Xmtp_KeystoreApi_V1_TopicMap.TopicData(serializedData: topicData))
		XCTAssertEqual("example.com/alice-bob-1", aliceConvo2.conversationID)

		// Now Alice should be able to load message using her saved key material.
		let messages = try await aliceConvo2.messages()
		XCTAssertEqual("Hello Bob", messages[1].body)
		XCTAssertEqual("Oh, hello Alice", messages[0].body)
	}

	func testDeterministicConversationCreation() async throws {
		let opt = ClientOptions(api: .init(env: .local, isSecure: false))
		let alice = try await Client.create(account: try PrivateKey.generate(), options: opt)
		let bob = try await Client.create(account: try PrivateKey.generate(), options: opt)

		// First Alice starts a conversation with Bob
		let context = InvitationV1.Context.with {
			$0.conversationID = "example.com/alice-bob-foo"
		}
		let c1 = try await alice.conversations.newConversation(with: bob.address, context: context)
		_ = try await c1.send(text: "Hello Bob")
		try await delayToPropagate()

		// Then Alice starts the same conversation (with Bob, same conversation ID)
		let c2 = try await alice.conversations.newConversation(with: bob.address, context: context)
		_ = try await c2.send(text: "And another one")
		try await delayToPropagate()

		// Alice should see the same topic and keyMaterial for both conversations.
		XCTAssertEqual(c1.topic, c2.topic)
		XCTAssertEqual(
				try c1.toTopicData().invitation.aes256GcmHkdfSha256.keyMaterial,
				try c2.toTopicData().invitation.aes256GcmHkdfSha256.keyMaterial)

		// And Bob should only see the one conversation.
		let bobConvos = try await bob.conversations.list()
		XCTAssertEqual(1, bobConvos.count)
		XCTAssertEqual(c1.topic, bobConvos[0].topic)
		XCTAssertEqual("example.com/alice-bob-foo", bobConvos[0].conversationID)

		let bobMessages = try await bobConvos[0].messages()
		XCTAssertEqual(2, bobMessages.count)
		XCTAssertEqual("Hello Bob", bobMessages[1].body)
		XCTAssertEqual("And another one", bobMessages[0].body)
	}

	func testStreamMessagesInV1Conversation() async throws {
		let opt = ClientOptions(api: .init(env: .local, isSecure: false))
		let alice = try await Client.create(account: try PrivateKey.generate(), options: opt)
		let bob = try await Client.create(account: try PrivateKey.generate(), options: opt)
        try await alice.publishUserContact(legacy: true)
		try await bob.publishUserContact(legacy: true)
        try await delayToPropagate()

		let aliceConversation = try await alice.conversations.newConversation(with: bob.address)
		try await aliceConversation.send(content: "greetings")
        try await delayToPropagate()

		let transcript = TestTranscript()

		let bobConversation = try await bob.conversations.newConversation(with: alice.address)

		XCTAssertEqual(bobConversation.topic.description, aliceConversation.topic.description)

		Task(priority: .userInitiated) {
			for try await message in bobConversation.streamMessages() {
				await transcript.add(message.body)
			}
		}

		try await aliceConversation.send(content: "hi bob")
		try await delayToPropagate()
		try await bobConversation.send(content: "hi alice")
        try await delayToPropagate()

        let messages = await transcript.messages
		XCTAssertEqual("hi bob", messages[0])
		XCTAssertEqual("hi alice", messages[1])
	}

	func testStreamMessagesInV2Conversation() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		let bobClient = try await Client.create(account: bob, options: clientOptions)

		let aliceConversation = try await aliceClient.conversations.newConversation(with: bob.walletAddress, context: .init(conversationID: "https://example.com/3"))

		let transcript = TestTranscript()

		let bobConversation = try await bobClient.conversations.newConversation(with: alice.walletAddress, context: .init(conversationID: "https://example.com/3"))

		XCTAssertEqual(bobConversation.topic, aliceConversation.topic)

		Task(priority: .userInitiated) {
            for try await message in bobConversation.streamMessages() {
                await transcript.add(message.body)
            }
		}
		try await aliceConversation.send(text: "hi bob")
		try await delayToPropagate()

        let messages = await transcript.messages
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("hi bob", messages[0])
	}

	func testStreamEphemeralInV1Conversation() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		try await aliceClient.publishUserContact(legacy: true)
		let bobClient = try await Client.create(account: bob, options: clientOptions)
		try await bobClient.publishUserContact(legacy: true)

		let expectation = expectation(description: "bob gets a streamed message")

		let convo = ConversationV1(client: bobClient, peerAddress: alice.address, sentAt: Date())

		Task(priority: .userInitiated) {
			for try await _ in convo.streamEphemeral() {
				expectation.fulfill()
			}
		}

		try await convo.send(content: "hi", options: .init(ephemeral: true))

		let messages = try await convo.messages()
		XCTAssertEqual(0, messages.count)

		await waitForExpectations(timeout: 3)
	}

	func testStreamEphemeralInV2Conversation() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		let bobClient = try await Client.create(account: bob, options: clientOptions)

		let aliceConversation = try await aliceClient.conversations.newConversation(with: bob.walletAddress, context: .init(conversationID: "https://example.com/3"))

		let expectation = expectation(description: "bob gets a streamed message")

		guard case let .v2(bobConversation) = try await
			bobClient.conversations.newConversation(with: alice.walletAddress, context: .init(conversationID: "https://example.com/3"))
		else {
			XCTFail("Did not create v2 convo")
			return
		}

		XCTAssertEqual(bobConversation.topic, aliceConversation.topic)

		Task(priority: .userInitiated) {
			for try await _ in bobConversation.streamEphemeral() {
				expectation.fulfill()
			}
		}

		try await aliceConversation.send(content: "hi", options: .init(ephemeral: true))

		let messages = try await aliceConversation.messages()
		XCTAssertEqual(0, messages.count)

		await waitForExpectations(timeout: 3)
	}

	func testCanPaginateV1Messages() async throws {
        try TestConfig.skipIfNotRunningLocalNodeTests()

		let bob = try FakeWallet.generate()
		let alice = try FakeWallet.generate()

		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let bobClient = try await Client.create(account: bob, options: options)

		// Publish alice's contact
		_ = try await Client.create(account: alice, options: options)

		let convo = ConversationV1(client: bobClient, peerAddress: alice.address, sentAt: Date())

		// Say this message is sent in the past
		try await convo.send(content: "first")
        try await delayToPropagate()
		try await convo.send(content: "second")
        try await delayToPropagate()

		var messages = try await convo.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
		XCTAssertEqual("second", messages[0].body) // most-recent first
        let secondMessageSent = messages[0].sent
//
//        messages = try await convo.messages(limit: 1, before: secondMessageSent)
//		XCTAssertEqual(1, messages.count)
//        XCTAssertEqual("first", messages[0].body)
//        let firstMessageSent = messages[0].sent
//
//		messages = try await convo.messages(limit: 1, after: firstMessageSent)
//		XCTAssertEqual(1, messages.count)
//		XCTAssertEqual("second", messages[0].body)
	}

	func testCanPaginateV2Messages() async throws {
		let bob = try FakeWallet.generate()
		let alice = try FakeWallet.generate()

		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let bobClient = try await Client.create(account: bob, options: options)

		// Publish alice's contact
		_ = try await Client.create(account: alice, options: options)

		guard case let .v2(convo) = try await bobClient.conversations.newConversation(with: alice.address) else {
			XCTFail("Did not get a v2 convo")
			return
		}

		// Say this message is sent in the past
		let tenSecondsAgo = Date().addingTimeInterval(-10)
		try await convo.send(content: "10 seconds ago", sentAt: tenSecondsAgo)
		try await convo.send(content: "now")

		let messages = try await convo.messages(limit: 10)
		XCTAssertEqual(2, messages.count)
		let nowMessage = messages[0]
		XCTAssertEqual("now", nowMessage.body)

		let messages2 = try await convo.messages(limit: 1, before: nowMessage.sent)
		XCTAssertEqual(1, messages2.count)
		let tenSecondsAgoMessage = messages2[0]
		XCTAssertEqual("10 seconds ago", tenSecondsAgoMessage.body)

		let messages3 = try await convo.messages(limit: 1, after: tenSecondsAgoMessage.sent)
		XCTAssertEqual(1, messages3.count)
		let nowMessage2 = messages3[0]
		XCTAssertEqual("now", nowMessage2.body)

        let messagesAsc = try await convo.messages(direction: .ascending)
        XCTAssertEqual("10 seconds ago", messagesAsc[0].body)

        let messagesDesc = try await convo.messages(direction: .descending)
        XCTAssertEqual("now", messagesDesc[0].body)
	}

    func testStreamingMessagesShouldBeReceived() async throws {
        let alice = try await Client.create(account: try FakeWallet.generate(),
                                                options: ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false)))
        let bob = try await Client.create(account: try FakeWallet.generate(),
                                          options: ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false)))
        let transcript = TestTranscript()
        Task(priority: .userInitiated) {
            for try await message in try await alice.conversations.streamAllMessages() {
                await transcript.add(message.body)
            }
        }
        let c1 = try await bob.conversations.newConversation(with: alice.address)
        try await delayToPropagate()
        _ = try await c1.send(text: "hello Alice")
        try await delayToPropagate()
        let messages = await transcript.messages
        XCTAssertEqual(1, messages.count)
        XCTAssertEqual("hello Alice", messages[0])
    }

    func testListingConversations() async throws {
        let alice = try await Client.create(account: try FakeWallet.generate(),
                                                options: ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false)))
        let bob = try await Client.create(account: try FakeWallet.generate(),
                                          options: ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false)))

        let c1 = try await bob.conversations.newConversation(
            with: alice.address,
            context: InvitationV1.Context.with {
                $0.conversationID = "example.com/alice-bob-1"
                $0.metadata["title"] = "First Chat"
        })
        try await c1.send(text: "hello Alice!")
        try await delayToPropagate()

        var aliceConvoList = try await alice.conversations.list()
        XCTAssertEqual(1, aliceConvoList.count)
        XCTAssertEqual("example.com/alice-bob-1", aliceConvoList[0].conversationID)

        let c2 = try await bob.conversations.newConversation(
            with: alice.address,
            context: InvitationV1.Context.with {
                $0.conversationID = "example.com/alice-bob-2"
                $0.metadata["title"] = "Second Chat"
            })
        try await c2.send(text: "hello again Alice!")
        try await delayToPropagate()

        aliceConvoList = try await alice.conversations.list()
        XCTAssertEqual(2, aliceConvoList.count)
//        XCTAssertEqual("example.com/alice-bob-2", aliceConvoList[0].conversationID)
//        XCTAssertEqual("example.com/alice-bob-1", aliceConvoList[1].conversationID)
    }

	// Test used to verify https://github.com/xmtp/xmtp-ios/issues/39 fix.
	func testExistingWallet() async throws {
		throw XCTSkip("manual only (requires dev network)")

		// Generated from JS script
		let keyBytes = Data([
			31, 116, 198, 193, 189, 122, 19, 254,
			191, 189, 211, 215, 255, 131, 171, 239,
			243, 33, 4, 62, 143, 86, 18, 195,
			251, 61, 128, 90, 34, 126, 219, 236,
		])

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = Data(try LibXMTP.publicKeyFromPrivateKeyK256(privateKeyBytes: keyBytes))

		let client = try await XMTPiOS.Client.create(account: key)
		XCTAssertEqual(client.apiClient.environment, .dev)

		let conversations = try await client.conversations.list()
		XCTAssertEqual(1, conversations.count)

		let message = try await conversations[0].messages().first
		XCTAssertEqual(message?.body, "hello")
	}

	func testCanStreamV2Conversations() async throws {
		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		let bobClient = try await Client.create(account: bob, options: clientOptions)

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await convo in try await bobClient.conversations.stream() {
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
		let newClient = try await Client.create(account: newWallet, options: clientOptions)

		guard case let .v2(conversation2) = try await bobClient.conversations.newConversation(with: newWallet.walletAddress) else {
			XCTFail("Did not create a v2 convo")
			return
		}

		try await conversation2.send(content: "hi from new wallet")

		await waitForExpectations(timeout: 3)
	}

	func testCanReadGzipCompressedMessages() async throws {
		throw XCTSkip("manual only (requires dev network)")

		let keyBytes = Data([
			225, 2, 36, 98, 37, 243, 68, 234,
			42, 126, 248, 246, 126, 83, 186, 197,
			204, 186, 19, 173, 51, 0, 64, 0,
			155, 8, 249, 247, 163, 185, 124, 159,
		])

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = Data(try LibXMTP.publicKeyFromPrivateKeyK256(privateKeyBytes: keyBytes))

		let client = try await XMTPiOS.Client.create(account: key)
		XCTAssertEqual(client.apiClient.environment, .dev)

		let convo = try await client.conversations.list()[0]
		let message = try await convo.messages()[0]

		XCTAssertEqual("hello gzip", try message.content())
	}

	func testCanReadZipCompressedMessages() async throws {
		throw XCTSkip("manual only (requires dev network)")

		let keyBytes = Data([
			60, 45, 240, 192, 223, 2, 14, 166,
			122, 65, 231, 31, 122, 178, 158, 137,
			192, 97, 139, 83, 133, 245, 149, 250,
			25, 125, 25, 11, 203, 97, 12, 200,
		])

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = Data(try LibXMTP.publicKeyFromPrivateKeyK256(privateKeyBytes: keyBytes))

		let client = try await XMTPiOS.Client.create(account: key)
		XCTAssertEqual(client.apiClient.environment, .dev)

		let convo = try await client.conversations.list()[0]
		let message = try await convo.messages().last!

		let swiftdata = Data("hello deflate".utf8) as NSData
		print("swift version: \((try swiftdata.compressed(using: .zlib) as Data).bytes)")

		XCTAssertEqual("hello deflate", try message.content())

		// Check that we can send as well
		try await convo.send(text: "hello deflate from swift again", options: .init(compression: .deflate))
	}

	func testCanLoadAllConversations() async throws {
		throw XCTSkip("manual only (requires dev network)")

		let keyBytes = Data([
			105, 207, 193, 11, 240, 115, 115, 204,
			117, 134, 201, 10, 56, 59, 52, 90,
			229, 103, 15, 66, 20, 113, 118, 137,
			44, 62, 130, 90, 30, 158, 182, 178,
		])

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = Data(try LibXMTP.publicKeyFromPrivateKeyK256(privateKeyBytes: keyBytes))


		let client = try await XMTPiOS.Client.create(account: key)

		let conversations = try await client.conversations.list()

		XCTAssertEqual(200, conversations.count)
	}

    // Helpers

    func delayToPropagate() async throws {
        try await Task.sleep(for: .milliseconds(500))
    }
}
