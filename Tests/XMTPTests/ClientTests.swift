//
//  ClientTests.swift
//
//
//  Created by Pat Nakajima on 11/22/22.
//

import Foundation

import XCTest
@testable import XMTP

class ClientTests: XCTestCase {
	func testTakesAWallet() async throws {
		let fakeWallet = try PrivateKey.generate()
		_ = try await Client.create(account: fakeWallet)
	}

	func testHasPrivateKeyBundleV1() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		XCTAssertEqual(1, client.privateKeyBundleV1.preKeys.count)

		let preKey = client.privateKeyBundleV1.preKeys[0]

		XCTAssert(preKey.publicKey.hasSignature, "prekey not signed")
	}

	func testCanBeCreatedWithBundle() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let bundle = client.privateKeyBundle
		let clientFromV1Bundle = try Client.from(bundle: bundle)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(client.privateKeyBundleV1.identityKey, clientFromV1Bundle.privateKeyBundleV1.identityKey)
		XCTAssertEqual(client.privateKeyBundleV1.preKeys, clientFromV1Bundle.privateKeyBundleV1.preKeys)
	}

	func testCanBeCreatedWithV1Bundle() async throws {
		let fakeWallet = try PrivateKey.generate()
		let client = try await Client.create(account: fakeWallet)

		let bundleV1 = client.v1keys
		let clientFromV1Bundle = try Client.from(v1Bundle: bundleV1)

		XCTAssertEqual(client.address, clientFromV1Bundle.address)
		XCTAssertEqual(client.privateKeyBundleV1.identityKey, clientFromV1Bundle.privateKeyBundleV1.identityKey)
		XCTAssertEqual(client.privateKeyBundleV1.preKeys, clientFromV1Bundle.privateKeyBundleV1.preKeys)
	}
}
