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
@testable import XMTP
import XMTPTestHelpers

@available(iOS 16, *)
final class IntegrationTests: XCTestCase {
	func testSaveKey() async throws {
		throw XCTSkip("integration only (requires local node)")

		let alice = try PrivateKey.generate()
		let identity = try PrivateKey.generate()

		let authorized = try await alice.createIdentity(identity)

		let authToken = try await authorized.createAuthToken()

		let api = try GRPCApiClient(environment: .local, secure: false)
		api.setAuthToken(authToken)

		let encryptedBundle = try await authorized.toBundle.encrypted(with: alice)

		var envelope = Envelope()
		envelope.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch) * 1_000_000
		envelope.message = try encryptedBundle.serializedData()

		try await api.publish(envelopes: [envelope])

		try await Task.sleep(nanoseconds: 2_000_000_000)

		let result = try await api.query(topics: [.userPrivateStoreKeyBundle(authorized.address)])
		XCTAssert(result.envelopes.count == 1)
	}

	func testPublishingAndFetchingContactBundlesWithWhileGeneratingKeys() async throws {
		throw XCTSkip("integration only (requires local node)")

		let aliceWallet = try PrivateKey.generate()
		let clientOptions = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let client = try await Client.create(account: aliceWallet, options: clientOptions)
		XCTAssertEqual(.local, client.apiClient.environment)

		let noContactYet = try await client.getUserContact(peerAddress: aliceWallet.walletAddress)
		XCTAssertNil(noContactYet)

		try await client.publishUserContact()

		let contact = try await client.getUserContact(peerAddress: aliceWallet.walletAddress)

		XCTAssertEqual(contact?.v1.keyBundle.identityKey.secp256K1Uncompressed, client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed)
		XCTAssert(contact?.v1.keyBundle.identityKey.hasSignature == true, "no signature")
		XCTAssert(contact?.v1.keyBundle.preKey.hasSignature == true, "pre key not signed")
	}

	func testPublishingAndFetchingContactBundlesWithSavedKeys() async throws {
		throw XCTSkip("integration only (requires local node)")

		let aliceWallet = try PrivateKey.generate()
		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)

		// Save keys
		let identity = try PrivateKey.generate()
		let authorized = try await aliceWallet.createIdentity(identity)
		let authToken = try await authorized.createAuthToken()
		var api = try GRPCApiClient(environment: .local, secure: false)
		api.setAuthToken(authToken)
		let encryptedBundle = try await PrivateKeyBundle(v1: alice).encrypted(with: aliceWallet)
		var envelope = Envelope()
		envelope.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch) * 1_000_000
		envelope.message = try encryptedBundle.serializedData()
		try await api.publish(envelopes: [envelope])
		// Done saving keys

		let clientOptions = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let client = try await Client.create(account: aliceWallet, options: clientOptions)
		XCTAssertEqual(.local, client.apiClient.environment)

		let noContactYet = try await client.getUserContact(peerAddress: aliceWallet.walletAddress)
		XCTAssertNil(noContactYet)

		try await client.publishUserContact()

		let contact = try await client.getUserContact(peerAddress: aliceWallet.walletAddress)

		XCTAssertEqual(contact?.v1.keyBundle.identityKey.secp256K1Uncompressed, client.privateKeyBundleV1.identityKey.publicKey.secp256K1Uncompressed)
		XCTAssert(contact?.v1.keyBundle.identityKey.hasSignature == true, "no signature")
		XCTAssert(contact?.v1.keyBundle.preKey.hasSignature == true, "pre key not signed")
	}

	func publishLegacyContact(client: XMTP.Client) async throws {
		var contactBundle = ContactBundle()
		contactBundle.v1.keyBundle = client.privateKeyBundleV1.toPublicKeyBundle()

		var envelope = Envelope()
		envelope.contentTopic = Topic.contact(client.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch * 1_000_000)
		envelope.message = try contactBundle.serializedData()

		try await client.publish(envelopes: [envelope])
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
		throw XCTSkip("integration only (requires local node)")

		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))

		let fakeContactWallet = try PrivateKey.generate()
		let fakeContactClient = try await Client.create(account: fakeContactWallet, options: options)
		try await fakeContactClient.publishUserContact()

		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: options)

		let contact = try await client.getUserContact(peerAddress: fakeContactWallet.walletAddress)!

		XCTAssertEqual(contact.walletAddress, fakeContactWallet.walletAddress)
		let privkeybundlev2 = try client.privateKeyBundleV1.toV2()
		let created = Date()

		var invitationContext = InvitationV1.Context()
		invitationContext.conversationID = "https://example.com/1"
		let invitationv1 = try InvitationV1.createRandom(context: invitationContext)
		let senderBundle = try client.privateKeyBundleV1.toV2()

		XCTAssertEqual(try senderBundle.identityKey.publicKey.recoverWalletSignerPublicKey().walletAddress, fakeWallet.address)
		let invitation = try SealedInvitation.createV1(
			sender: try client.privateKeyBundleV1.toV2(),
			recipient: try contact.toSignedPublicKeyBundle(),
			created: created,
			invitation: invitationv1
		)

		let inviteHeader = invitation.v1.header
		XCTAssertEqual(try inviteHeader.sender.walletAddress, fakeWallet.walletAddress)
		XCTAssertEqual(try inviteHeader.recipient.walletAddress, fakeContactWallet.walletAddress)

		let header = try SealedInvitationHeaderV1(serializedData: invitation.v1.headerBytes)
		let conversation = try ConversationV2.create(client: client, invitation: invitationv1, header: header)

		XCTAssertEqual(fakeContactWallet.walletAddress, conversation.peerAddress)
//
		do {
			try await conversation.send(content: "hello world")
		} catch {
			print("ERROR SENDING \(error)")
		}

		let conversationList = try await client.conversations.list()

		print("CONVO LIST \(conversationList)")

		guard case let .v2(recipientConversation) = conversationList.last else {
			XCTFail("No conversation found")
			return
		}

		let messages = try await recipientConversation.messages()

		if let message = messages.first {
			XCTAssertEqual("hello world", message.body)
		} else {
			XCTFail("no messages")
		}
	}

	func testStreamMessagesInV1Conversation() async throws {
		throw XCTSkip("integration only (requires local node)")

		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		try await aliceClient.publishUserContact(legacy: true)
		let bobClient = try await Client.create(account: bob, options: clientOptions)
		try await bobClient.publishUserContact(legacy: true)

		guard case let .v1(aliceConversation) = try await aliceClient.conversations.newConversation(with: bob.walletAddress) else {
			XCTFail("Did not create v1 convo")
			return
		}

		try await aliceConversation.send(content: "greetings")

		let expectation = expectation(description: "bob gets a streamed message")

		guard case let .v1(bobConversation) = try await bobClient.conversations.newConversation(with: alice.walletAddress) else {
			XCTFail("Did not get v1 convo")
			return
		}

		XCTAssertEqual(bobConversation.topic.description, aliceConversation.topic.description)

		Task(priority: .userInitiated) {
			for try await _ in bobConversation.streamMessages() {
				expectation.fulfill()
			}
		}

		try await aliceConversation.send(content: "hi bob")
		try await bobConversation.send(content: "hi alice")

		await waitForExpectations(timeout: 3)
	}

	func testStreamMessagesInV2Conversation() async throws {
		throw XCTSkip("integration only (requires local node)")

		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		let bobClient = try await Client.create(account: bob, options: clientOptions)

		let aliceConversation = try await aliceClient.conversations.newConversation(with: bob.walletAddress, context: .init(conversationID: "https://example.com/3"))

		let expectation = expectation(description: "bob gets a streamed message")

		let bobConversation = try await bobClient.conversations.newConversation(with: alice.walletAddress, context: .init(conversationID: "https://example.com/3"))

		XCTAssertEqual(bobConversation.topic, aliceConversation.topic)

		Task(priority: .userInitiated) {
			for try await _ in bobConversation.streamMessages() {
				expectation.fulfill()
			}
		}

		try await aliceConversation.send(text: "hi bob")

		await waitForExpectations(timeout: 3)
	}

	func testCanPaginateV1Messages() async throws {
		throw XCTSkip("integration only (requires local node)")

		let bob = try FakeWallet.generate()
		let alice = try FakeWallet.generate()

		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let bobClient = try await Client.create(account: bob, options: options)

		// Publish alice's contact
		_ = try await Client.create(account: alice, options: options)

		let convo = ConversationV1(client: bobClient, peerAddress: alice.address, sentAt: Date())

		// Say this message is sent in the past
		try await convo.send(content: "10 seconds ago", sentAt: Date().addingTimeInterval(-10))

		try await convo.send(content: "now")

		let messages = try await convo.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
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
	}

	func testCanPaginateV2Messages() async throws {
		throw XCTSkip("integration only (requires local node)")

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

		let messages = try await convo.messages(limit: 1)
		XCTAssertEqual(1, messages.count)
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
	}

	// Test used to verify https://github.com/xmtp/xmtp-ios/issues/39 fix.
	func testExistingWallet() async throws {
		throw XCTSkip("integration only (requires dev network)")

		// Generated from JS script
		let keyBytes: [UInt8] = [
			31, 116, 198, 193, 189, 122, 19, 254,
			191, 189, 211, 215, 255, 131, 171, 239,
			243, 33, 4, 62, 143, 86, 18, 195,
			251, 61, 128, 90, 34, 126, 219, 236,
		]

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: Data(keyBytes))

		let client = try await XMTP.Client.create(account: key)
		XCTAssertEqual(client.apiClient.environment, .dev)

		let conversations = try await client.conversations.list()
		XCTAssertEqual(1, conversations.count)

		let message = try await conversations[0].messages().first
		XCTAssertEqual(message?.body, "hello")
	}

	func testCanStreamV2Conversations() async throws {
		throw XCTSkip("integration only (requires local node)")

		let alice = try PrivateKey.generate()
		let bob = try PrivateKey.generate()

		let clientOptions = ClientOptions(api: .init(env: .local, isSecure: false))
		let aliceClient = try await Client.create(account: alice, options: clientOptions)
		let bobClient = try await Client.create(account: bob, options: clientOptions)

		let expectation1 = expectation(description: "got a conversation")
		expectation1.expectedFulfillmentCount = 2

		Task(priority: .userInitiated) {
			for try await convo in bobClient.conversations.stream() {
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
		throw XCTSkip("integration only (requires dev network)")

		let keyBytes: [UInt8] = [
			225, 2, 36, 98, 37, 243, 68, 234,
			42, 126, 248, 246, 126, 83, 186, 197,
			204, 186, 19, 173, 51, 0, 64, 0,
			155, 8, 249, 247, 163, 185, 124, 159,
		]

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: Data(keyBytes))

		let client = try await XMTP.Client.create(account: key)
		XCTAssertEqual(client.apiClient.environment, .dev)

		let convo = try await client.conversations.list()[0]
		let message = try await convo.messages()[0]

		XCTAssertEqual("hello gzip", try message.content())
	}

	func testCanReadZipCompressedMessages() async throws {
		throw XCTSkip("integration only (requires dev network)")

		let keyBytes: [UInt8] = [
			60, 45, 240, 192, 223, 2, 14, 166,
			122, 65, 231, 31, 122, 178, 158, 137,
			192, 97, 139, 83, 133, 245, 149, 250,
			25, 125, 25, 11, 203, 97, 12, 200,
		]

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: Data(keyBytes))

		let client = try await XMTP.Client.create(account: key)
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
		throw XCTSkip("integration only (requires dev network)")

		let keyBytes: [UInt8] = [
			105, 207, 193, 11, 240, 115, 115, 204,
			117, 134, 201, 10, 56, 59, 52, 90,
			229, 103, 15, 66, 20, 113, 118, 137,
			44, 62, 130, 90, 30, 158, 182, 178,
		]

		var key = PrivateKey()
		key.secp256K1.bytes = Data(keyBytes)
		key.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: Data(keyBytes))

		let client = try await XMTP.Client.create(account: key)

		let conversations = try await client.conversations.list()

		XCTAssertEqual(200, conversations.count)
	}
}
