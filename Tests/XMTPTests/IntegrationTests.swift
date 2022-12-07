//
//  IntegrationTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import Foundation
import secp256k1
import WalletConnectSwift
import web3
import XCTest
@testable import XMTP

class CallbackyConnection: WCWalletConnection {
	var onConnect: (() -> Void)?

	override func client(_ client: WalletConnectSwift.Client, didConnect session: WalletConnectSwift.Session) {
		super.client(client, didConnect: session)
		onConnect?()
	}

	override func preferredConnectionMethod() throws -> WalletConnectionMethodType {
		return WalletManualConnectionMethod(redirectURI: walletConnectURL?.asURL.absoluteString ?? "").type
	}
}

@available(iOS 16, *)
final class IntegrationTests: XCTestCase {
	func testSaveKey() async throws {
		throw XCTSkip("integration only (requires local node)")

		let alice = try PrivateKey.generate()
		let identity = try PrivateKey.generate()

		let authorized = try await alice.createIdentity(identity)

		let authToken = try await authorized.createAuthToken()

		var api = try GRPCApiClient(environment: .local, secure: false)
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

	func testWalletSaveKey() async throws {
		throw XCTSkip("integration only (requires local node)")

		let connection = CallbackyConnection()
		let wallet = try Account(connection: connection)

		let expectation = expectation(description: "connected")

		connection.onConnect = {
			expectation.fulfill()
		}

		guard case let .manual(url) = try connection.preferredConnectionMethod() else {
			XCTFail("No WC URL")
			return
		}

		print("Open in mobile safari: \(url)")
		try await connection.connect()

		wait(for: [expectation], timeout: 60)

		let privateKey = try PrivateKey.generate()
		let authorized = try await wallet.createIdentity(privateKey)
		let authToken = try await authorized.createAuthToken()

		var api = try GRPCApiClient(environment: .local, secure: false)
		api.setAuthToken(authToken)

		let encryptedBundle = try await authorized.toBundle.encrypted(with: wallet)

		var envelope = Envelope()
		envelope.contentTopic = Topic.userPrivateStoreKeyBundle(authorized.address).description
		envelope.timestampNs = UInt64(Date().millisecondsSinceEpoch) * 1_000_000
		envelope.message = try encryptedBundle.serializedData()

		try await api.publish(envelopes: [envelope])

		try await Task.sleep(nanoseconds: 2_000_000_000)

		let result = try await api.query(topics: [.userPrivateStoreKeyBundle("0xE2c094aB885170B56A811f0c8b5FeDC4a2565575")])
		XCTAssert(result.envelopes.count >= 1)
	}

	func testPublishingAndFetchingContactBundlesWithWhileGeneratingKeys() async throws {
		throw XCTSkip("integration only (requires local node)")

		let aliceWallet = try PrivateKey.generate()
		let alice = try await PrivateKeyBundleV1.generate(wallet: aliceWallet)

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

	func testCanReceiveMessagesFromJS() async throws {
		throw XCTSkip("integration only (requires local node)")

		//  Uncomment these lines to generate a new wallet to test with the JS sdk
//		var wallet = try PrivateKey.generate()
//		print("wallet bytes \(wallet.secp256K1.bytes.bytes)")
//		print("NEW address \(wallet.walletAddress)")

		var wallet = PrivateKey()
		wallet.secp256K1.bytes = Data([8, 103, 164, 168, 62, 63, 146, 40, 194, 165, 137, 89, 228, 126, 62, 81, 202, 187, 231, 21, 154, 42, 144, 172, 79, 70, 155, 235, 33, 116, 121, 120])
		wallet.publicKey.secp256K1Uncompressed.bytes = try KeyUtil.generatePublicKey(from: wallet.secp256K1.bytes)
		print("OUR ADDRESS: \(wallet.walletAddress)")

		let options = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let client = try await Client.create(account: wallet, options: options)

		try await client.publishUserContact()

		let convos = try await client.conversations.list()

		guard let convo = convos.first else {
			XCTFail("No conversations")
			return
		}

		var messages: [DecodedMessage] = []

		switch convo {
		case let .v1(conversation):
			messages = try await conversation.messages()
		case let .v2(conversation):
			messages = try await conversation.messages()
		}

		try await convo.send(text: "hello from swift")
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
		let conversations = Conversations(client: client)

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

		let recipBundle = privkeybundlev2.getPublicKeyBundle()
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
}
