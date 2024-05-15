//
//  ClientTests.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation

import XCTest
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(iOS 15, *)
class ClientTests: XCTestCase {
	func testTakesAWallet() async throws {
			try TestConfig.skip(because: "run manually against dev")
		let fakeWallet = try PrivateKey.generate()
		_ = try await Client.create(account: fakeWallet)
	}

	func testPassingSavedKeysWithNoSignerWithMLSErrors() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()

		do {
			let client = try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					mlsAlpha: true,
					mlsEncryptionKey: key
				)
			)
		} catch {
			XCTAssert(error.localizedDescription.contains("no keys"))
		}
	}

	func testPassingSavedKeysWithMLS() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let client = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true,
				mlsEncryptionKey: key
			)
		)

		let keys = client.privateKeyBundle
		let otherClient = try await Client.from(
			bundle: keys,
			options: .init(
				api: .init(env: .local, isSecure: false),
				// Should not need to pass the signer again
				mlsAlpha: true,
				mlsEncryptionKey: key
			)
		)

		XCTAssertEqual(client.address, otherClient.address)
	}

	func testPassingMLSEncryptionKey() async throws {
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		_ = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true,
				mlsEncryptionKey: key
			)
		)

		do {
			_ = try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					mlsAlpha: true,
					mlsEncryptionKey: nil // No key should error
				)
			)

			XCTFail("did not throw")
		} catch {
			XCTAssert(true)
		}
	}
	
	func testPassingMLSEncryptionKeyAndDatabasePath() async throws {		
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)
		let documentsURL = try
			FileManager.default.url(
				for: .documentDirectory,
				in: .userDomainMask,
				appropriateFor: nil,
				create: false
			)
		
		let dbPath = documentsURL.appendingPathComponent("xmtp-\(bo.walletAddress).db3").path
		
		let client = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true,
				mlsEncryptionKey: key,
				mlsDbPath: dbPath
			)
		)
		
		let keys = client.privateKeyBundle
		let bundleClient = try await Client.from(
			bundle: keys,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true,
				mlsEncryptionKey: key,
				mlsDbPath: dbPath
			)
		)

		XCTAssertEqual(client.address, bundleClient.address)
		XCTAssert(!client.installationID.isEmpty)
		
		await assertThrowsAsyncError(
			_ = try await Client.from(
				bundle: keys,
				options: .init(
					api: .init(env: .local, isSecure: false),
					mlsAlpha: true,
					mlsEncryptionKey: nil,
					mlsDbPath: dbPath
				)
			)
		)
		
		await assertThrowsAsyncError(
			_ = try await Client.from(
				bundle: keys,
				options: .init(
					api: .init(env: .local, isSecure: false),
					mlsAlpha: true,
					mlsEncryptionKey: key,
					mlsDbPath: nil
				)
			)
		)
	}
	
	func testCanDeleteDatabase() async throws {
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		var boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true
			)
		)
	
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true
			)
		)

		_ = try await boClient.conversations.newGroup(with: [alixClient.address])
		try await boClient.conversations.sync()

		var groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 1)

		try boClient.deleteLocalDatabase()

		boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				mlsAlpha: true
			)
		)

		try await boClient.conversations.sync()
		groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 0)
	}

	func testCanMessage() async throws {
		let fixtures = await fixtures()
		let notOnNetwork = try PrivateKey.generate()

		let canMessage = try await fixtures.aliceClient.canMessage(fixtures.bobClient.address)
		let cannotMessage = try await fixtures.aliceClient.canMessage(notOnNetwork.address)
		XCTAssertTrue(canMessage)
		XCTAssertFalse(cannotMessage)
	}

		func testStaticCanMessage() async throws {
				try TestConfig.skip(because: "run manually against local")
				let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))

				let aliceWallet = try PrivateKey.generate()
				let notOnNetwork = try PrivateKey.generate()
				let alice = try await Client.create(account: aliceWallet, options: opts)

				let canMessage = try await Client.canMessage(alice.address, options: opts)
				let cannotMessage = try await Client.canMessage(notOnNetwork.address, options: opts)
				XCTAssertTrue(canMessage)
				XCTAssertFalse(cannotMessage)
		}

	func testHasPrivateKeyBundleV1() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, apiClient: FakeApiClient())

		XCTAssertEqual(1, client.privateKeyBundleV1.preKeys.count)

		let preKey = client.privateKeyBundleV1.preKeys[0]

		XCTAssert(preKey.publicKey.hasSignature, "prekey not signed")
	}

	func testCanBeCreatedWithBundle() async throws {
				try TestConfig.skip(because: "run manually against dev")
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let bundle = client.privateKeyBundle
		let clientFromV1Bundle = try await Client.from(bundle: bundle)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(client.privateKeyBundleV1.identityKey, clientFromV1Bundle.privateKeyBundleV1.identityKey)
		XCTAssertEqual(client.privateKeyBundleV1.preKeys, clientFromV1Bundle.privateKeyBundleV1.preKeys)
	}

	func testCanBeCreatedWithV1Bundle() async throws {
				try TestConfig.skip(because: "run manually against dev")
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let bundleV1 = client.v1keys
		let clientFromV1Bundle = try await Client.from(v1Bundle: bundleV1)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(client.privateKeyBundleV1.identityKey, clientFromV1Bundle.privateKeyBundleV1.identityKey)
		XCTAssertEqual(client.privateKeyBundleV1.preKeys, clientFromV1Bundle.privateKeyBundleV1.preKeys)
	}

	func testCanAccessPublicKeyBundle() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let publicKeyBundle = client.keys.getPublicKeyBundle()
		XCTAssertEqual(publicKeyBundle, client.publicKeyBundle)
	}

	func testCanSignWithPrivateIdentityKey() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let digest = Util.keccak256(Data("hello world".utf8))
		let signature = try await client.keys.identityKey.sign(digest)

		let recovered = try KeyUtilx.recoverPublicKeyKeccak256(from: signature.rawData, message: Data("hello world".utf8))

		XCTAssertEqual(recovered, client.keys.identityKey.publicKey.secp256K1Uncompressed.bytes)
	}

	func testPreEnableIdentityCallback() async throws {
		let fakeWallet = try PrivateKey.generate()
		let expectation = XCTestExpectation(description: "preEnableIdentityCallback is called")

		let preEnableIdentityCallback: () async throws -> Void = {
				print("preEnableIdentityCallback called")
				expectation.fulfill()
		}

		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false), preEnableIdentityCallback: preEnableIdentityCallback )
		do {
			_ = try await Client.create(account: fakeWallet, options: opts)
			await XCTWaiter().fulfillment(of: [expectation], timeout: 30)
		} catch {
			XCTFail("Error: \(error)")
		}
	}

	func testPreCreateIdentityCallback() async throws {
		let fakeWallet = try PrivateKey.generate()
		let expectation = XCTestExpectation(description: "preCreateIdentityCallback is called")

		let preCreateIdentityCallback: () async throws -> Void = {
				print("preCreateIdentityCallback called")
				expectation.fulfill()
		}

		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false), preCreateIdentityCallback: preCreateIdentityCallback )
		do {
			_ = try await Client.create(account: fakeWallet, options: opts)
			await XCTWaiter().fulfillment(of: [expectation], timeout: 30)
		} catch {
			XCTFail("Error: \(error)")
		}
	}
}
