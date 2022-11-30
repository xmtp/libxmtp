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
		throw XCTSkip("integration only")

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
		throw XCTSkip("integration only")

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
		throw XCTSkip("integration only")

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
		throw XCTSkip("integration only")

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
}
