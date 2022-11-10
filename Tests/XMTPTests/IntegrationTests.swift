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
}
