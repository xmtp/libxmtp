//
//  ApiClientTests.swift
//
//
//  Created by Pat Nakajima on 11/17/22.
//

import secp256k1
import XCTest
@testable import XMTP

final class ApiClientTests: XCTestCase {
	func testHelloWorld() throws {
		_ = try ApiClient(environment: .local)
	}
}
