//
//  SignatureTests.swift
//
//
//  Created by Pat Nakajima on 11/27/22.
//

import CryptoKit
import XCTest
@testable import XMTPiOS

class SignatureTests: XCTestCase {
	func testVerify() async throws {
		let digest = SHA256.hash(data: Data("Hello world".utf8))
		let signingKey = try PrivateKey.generate()
		let signature = try await signingKey.sign(Data(digest))
		XCTAssert(try signature.verify(signedBy: signingKey.publicKey, digest: Data("Hello world".utf8)))
	}
    
    func testConsentProofText() {
        let timestamp = UInt64(1581663600000)
        let exampleAddress = "0x1234567890abcdef";
        let text = Signature.consentProofText(peerAddress: exampleAddress, timestamp: timestamp)
        let expected = "XMTP : Grant inbox consent to sender\n\nCurrent Time: Fri, 14 Feb 2020 07:00:00 GMT\nFrom Address: 0x1234567890abcdef\n\nFor more info: https://xmtp.org/signatures/"

        XCTAssertEqual(text, expected)
    }
}
