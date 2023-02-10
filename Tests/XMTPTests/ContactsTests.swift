//
//  ContactsTests.swift
//
//
//  Created by Pat Nakajima on 12/8/22.
//

import XCTest
import XMTPTestHelpers
@testable import XMTP

@available(iOS 15, *)
class ContactsTests: XCTestCase {
	func testCanFindContact() async throws {
		let fixtures = await fixtures()

		try await fixtures.bobClient.ensureUserContactPublished()
		guard let contactBundle = try await fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress) else {
			XCTFail("did not find contact bundle")
			return
		}

		XCTAssertEqual(contactBundle.walletAddress, fixtures.bob.walletAddress)
	}

	func testCachesContacts() async throws {
		let fixtures = await fixtures()

		try await fixtures.bobClient.ensureUserContactPublished()

		// Look up the first time
		_ = try await fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress)

		try await fixtures.fakeApiClient.assertNoQuery {
			guard let contactBundle = try await fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress) else {
				XCTFail("did not find contact bundle")
				return
			}

			XCTAssertEqual(contactBundle.walletAddress, fixtures.bob.walletAddress)
		}

		XCTAssert(fixtures.aliceClient.contacts.has(fixtures.bob.walletAddress))
	}
}
