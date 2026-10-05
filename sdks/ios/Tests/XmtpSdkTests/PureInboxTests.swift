import XCTest
import XmtpSdk

final class PureInboxTests: XCTestCase {
	func testCanonicalInboxVectorsKeepDefaultAndWideNonces() throws {
		let identity = PublicIdentity(identifier: "0xabcdef0000000000000000000000000000000000", kind: .ethereum)
		let nonces: [UInt64] = [0, 1, 9_007_199_254_740_993, .max]
		let expected = [
			"139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
			"f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279",
			"7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
			"00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
		]
		for (nonce, inboxId) in zip(nonces, expected) {
			XCTAssertEqual(try generateInboxId(identity: identity, nonce: nonce), inboxId)
		}
		XCTAssertEqual(try generateInboxId(identity: identity), expected[0])
		XCTAssertEqual(try generateInboxId(identity: identity, nonce: nil), expected[0])
	}

	func testPasskeyInboxVectorCrossesNativeBoundary() throws {
		let identity = PublicIdentity(identifier: "abcdef", kind: .passkey)
		XCTAssertEqual(
			try generateInboxId(identity: identity),
			"e26bbe40a904acb658e0dd48f4031811b662ce4e6238eef5c46f5bb92550713a",
		)
	}
}
