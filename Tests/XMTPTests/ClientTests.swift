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
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		_ = try await Client.create(account: fakeWallet, options: opts)
	}

	func testPassingSavedKeysWithNoSignerWithMLSErrors() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()

		do {
			let client = try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					enableV3: true,
					encryptionKey: key
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
				enableV3: true,
				encryptionKey: key
			)
		)

		let keys = try client.privateKeyBundle
		let otherClient = try await Client.from(
			bundle: keys,
			options: .init(
				api: .init(env: .local, isSecure: false),
				// Should not need to pass the signer again
				enableV3: true,
				encryptionKey: key
			)
		)

		XCTAssertEqual(client.address, otherClient.address)
	}

	func testPassingencryptionKey() async throws {
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		_ = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)

		do {
			_ = try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					enableV3: true,
					encryptionKey: nil // No key should error
				)
			)

			XCTFail("did not throw")
		} catch {
			XCTAssert(true)
		}
	}
	
	func testCanDeleteDatabase() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		var boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)
	
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)

		_ = try await boClient.conversations.newGroup(with: [alixClient.address])
		try await boClient.conversations.sync()

		var groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 1)

		assert(!boClient.dbPath.isEmpty)
		try boClient.deleteLocalDatabase()

		boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)

		try await boClient.conversations.sync()
		groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 0)
	}
	
	func testCanDropReconnectDatabase() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		var boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)
	
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)

		_ = try await boClient.conversations.newGroup(with: [alixClient.address])
		try await boClient.conversations.sync()

		var groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 1)

		try boClient.dropLocalDatabaseConnection()

		await assertThrowsAsyncError(try await boClient.conversations.groups())

		try await boClient.reconnectLocalDatabase()

		groupCount = try await boClient.conversations.groups().count
		XCTAssertEqual(groupCount, 1)
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
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: opts)

		XCTAssertEqual(1, try client.v1keys.preKeys.count)

		let preKey = try client.v1keys.preKeys[0]

		XCTAssert(preKey.publicKey.hasSignature, "prekey not signed")
	}

	func testCanBeCreatedWithBundle() async throws {
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: opts)

		let bundle = try client.privateKeyBundle
		let clientFromV1Bundle = try await Client.from(bundle: bundle, options: opts)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(try client.v1keys.identityKey, try clientFromV1Bundle.v1keys.identityKey)
		XCTAssertEqual(try client.v1keys.preKeys, try clientFromV1Bundle.v1keys.preKeys)
	}

	func testCanBeCreatedWithV1Bundle() async throws {
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: opts)

		let bundleV1 = try client.v1keys
		let clientFromV1Bundle = try await Client.from(v1Bundle: bundleV1, options: opts)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(try client.v1keys.identityKey, try clientFromV1Bundle.v1keys.identityKey)
		XCTAssertEqual(try client.v1keys.preKeys, try clientFromV1Bundle.v1keys.preKeys)
	}

	func testCanAccessPublicKeyBundle() async throws {
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: opts)

		let publicKeyBundle = try client.keys.getPublicKeyBundle()
		XCTAssertEqual(publicKeyBundle, try client.publicKeyBundle)
	}

	func testCanSignWithPrivateIdentityKey() async throws {
		let opts = ClientOptions(api: ClientOptions.Api(env: .local, isSecure: false))
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet, options: opts)

		let digest = Util.keccak256(Data("hello world".utf8))
		let signature = try await client.keys.identityKey.sign(digest)

		let recovered = try KeyUtilx.recoverPublicKeyKeccak256(from: signature.rawData, message: Data("hello world".utf8))
		let bytes = try client.keys.identityKey.publicKey.secp256K1Uncompressed.bytes
		XCTAssertEqual(recovered, bytes)
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
    
    func testPreAuthenticateToInboxCallback() async throws {
        let fakeWallet = try PrivateKey.generate()
        let expectation = XCTestExpectation(description: "preAuthenticateToInboxCallback is called")
        let key = try Crypto.secureRandomBytes(count: 32)

        let preAuthenticateToInboxCallback: () async throws -> Void = {
                print("preAuthenticateToInboxCallback called")
                expectation.fulfill()
        }

        let opts = ClientOptions(
            api: ClientOptions.Api(env: .local, isSecure: false),
            preAuthenticateToInboxCallback: preAuthenticateToInboxCallback,
            enableV3: true,
            encryptionKey: key
        )
        do {
            _ = try await Client.create(account: fakeWallet, options: opts)
            await XCTWaiter().fulfillment(of: [expectation], timeout: 30)
        } catch {
            XCTFail("Error: \(error)")
        }
    }
	
	func testPassingencryptionKeyAndDatabaseDirectory() async throws {
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		let client = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let keys = try client.privateKeyBundle
		let bundleClient = try await Client.from(
			bundle: keys,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		XCTAssertEqual(client.address, bundleClient.address)
		XCTAssertEqual(client.dbPath, bundleClient.dbPath)
		XCTAssert(!client.installationID.isEmpty)

		await assertThrowsAsyncError(
			_ = try await Client.from(
				bundle: keys,
				options: .init(
					api: .init(env: .local, isSecure: false),
					enableV3: true,
					encryptionKey: nil,
					dbDirectory: "xmtp_db"
				)
			)
		)

		await assertThrowsAsyncError(
			_ = try await Client.from(
				bundle: keys,
				options: .init(
					api: .init(env: .local, isSecure: false),
					enableV3: true,
					encryptionKey: key,
					dbDirectory: nil
				)
			)
		)
	}
	
	func testEncryptionKeyCanDecryptCorrectly() async throws {
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		let boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)
		
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let group = try await boClient.conversations.newGroup(with: [alixClient.address])
		
		let key2 = try Crypto.secureRandomBytes(count: 32)
		await assertThrowsAsyncError(
			try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					enableV3: true,
					encryptionKey: key2,
					dbDirectory: "xmtp_db"
				)
			)
		)
	}
	
	func testCanGetAnInboxIdFromAddress() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		let boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)
	
		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				enableV3: true,
				encryptionKey: key
			)
		)
		let boInboxId = try await alixClient.inboxIdFromAddress(address: boClient.address)
		XCTAssertEqual(boClient.inboxID, boInboxId)
	}
	
	func testCreatesAV3Client() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			   enableV3: true,
			   encryptionKey: key
		   )


		let inboxId = try await Client.getOrCreateInboxId(options: options, address: alix.address)
		let alixClient = try await Client.create(
			account: alix,
			options: options
		)

		XCTAssertEqual(inboxId, alixClient.inboxID)
	}
	
	func testCreatesAPureV3Client() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			   enableV3: true,
			   encryptionKey: key
		   )


		let inboxId = try await Client.getOrCreateInboxId(options: options, address: alix.address)
		let alixClient = try await Client.createV3(
			account: alix,
			options: options
		)

		XCTAssertEqual(inboxId, alixClient.inboxID)
		
		let alixClient2 = try await Client.buildV3(
			address: alix.address,
			options: options
		)
		
		XCTAssertEqual(alixClient2.inboxID, alixClient.inboxID)
	}
	
	func testRevokesAllOtherInstallations() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			   enableV3: true,
			   encryptionKey: key
		   )

		let alixClient = try await Client.create(
			account: alix,
			options: options
		)
		try alixClient.dropLocalDatabaseConnection()
		try alixClient.deleteLocalDatabase()
		
		let alixClient2 = try await Client.create(
			account: alix,
			options: options
		)
		try alixClient2.dropLocalDatabaseConnection()
		try alixClient2.deleteLocalDatabase()

		let alixClient3 = try await Client.create(
			account: alix,
			options: options
		)
		
		let state = try await alixClient3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 3)
		XCTAssert(state.installations.first?.createdAt != nil)
		
		try await alixClient3.revokeAllOtherInstallations(signingKey: alix)
		
		let newState = try await alixClient3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(newState.installations.count, 1)
	}
	
	func testCreatesASCWClient() async throws {
		throw XCTSkip("TODO: Need to write a SCW local deploy with anvil")
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try FakeSCWWallet.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			   enableV3: true,
			   encryptionKey: key
		   )


		let inboxId = try await Client.getOrCreateInboxId(options: options, address: alix.address)
		
		let alixClient = try await Client.createV3(
			account: alix,
			options: options
		)
		
		let alixClient2 = try await Client.buildV3(address: alix.address, options: options)
		XCTAssertEqual(inboxId, alixClient.inboxID)
		XCTAssertEqual(alixClient2.inboxID, alixClient.inboxID)

	}
}
