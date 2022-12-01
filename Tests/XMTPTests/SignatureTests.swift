//
//  SignatureTests.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import CryptoKit
import XCTest
@testable import XMTP

class SignatureTests: XCTestCase {
	func testVerify() async throws {
		let digest = SHA256.hash(data: Data("Hello world".utf8))
		let signingKey = try PrivateKey.generate()
		let signature = try await signingKey.sign(Data(digest))

		XCTAssert(try signature.verify(signedBy: signingKey.publicKey, digest: Data("Hello world".utf8)))
	}
}
