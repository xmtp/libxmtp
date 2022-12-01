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

		var api = try ApiClient(environment: .local, secure: false)
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

		var api = try ApiClient(environment: .local, secure: false)
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
		var api = try ApiClient(environment: .local, secure: false)
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
		throw XCTSkip("integration only (requires dev network)")

		//  Uncomment these lines to generate a new wallet to test with the JS sdk
//		var wallet = try PrivateKey.generate()
//		print("wallet bytes \(wallet.secp256K1.bytes.bytes)")
//		print("NEW address \(wallet.walletAddress)")

		var wallet = PrivateKey()
		wallet.secp256K1.bytes = Data([65, 48, 96, 54, 186, 123, 188, 219, 77, 158, 158, 62, 89, 35, 88, 88, 118, 11, 79, 23, 239, 48, 25, 113, 195, 230, 114, 240, 243, 180, 122, 135])
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

		XCTAssert(messages.count > 0, "did not find messages")

		let contact = try await client.getUserContact(peerAddress: "0x8B26203eDc935Ab291ABC67Cd343c40970B3c1d3")!
		let invitation = try InvitationV1.createRandom()
		let created = Date()
		let sealedInvitation = try await client.conversations.sendInvitation(
			recipient: contact.toSignedPublicKeyBundle(),
			invitation: invitation,
			created: created
		)

		let header = try SealedInvitationHeaderV1(serializedData: sealedInvitation.v1.headerBytes)
		let conversation = ConversationV1(client: client, peerAddress: "0x8B26203eDc935Ab291ABC67Cd343c40970B3c1d3", sentAt: Date())
//
//		do {
//			try await conversation.send(content: "and another one...")
//		} catch {
//			print("ERROR SENDING \(error)")
//		}
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
		let invitationv1 = try InvitationV1.createRandom()
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
		let sealedInvitation = try await conversations.sendInvitation(
			recipient: recipBundle,
			invitation: invitationv1,
			created: created
		)

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
}
