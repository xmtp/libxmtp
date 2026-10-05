import XCTest
import XmtpSdk

final class PureInboxTests: XCTestCase {
	func testCanonicalInboxVectorsKeepDefaultAndWideNonces() throws {
		let nonces: [UInt64] = [0, 1, 9_007_199_254_740_993, .max]
		let cases: [(PublicIdentity, [String])] = [
			(PublicIdentity(identifier: "0xabcdef0000000000000000000000000000000000", kind: .ethereum), [
				"139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
				"f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279",
				"7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
				"00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
			]),
			(PublicIdentity(identifier: "abcdef", kind: .passkey), [
				"e26bbe40a904acb658e0dd48f4031811b662ce4e6238eef5c46f5bb92550713a",
				"ac9f830ae6cf2299ba293dd4cec3be0d87a88e6a8fbfe5015de6fffd11d79b6e",
				"ef91da01728a1d16593d300a7a699d6a7831c43e00916a6b199f8244f207637d",
				"469fa9bb87114e117a27305350728cb2a4f85fdae9b5ccaef3b702f879dd9ae4",
			]),
		]
		for (identity, expected) in cases {
			for (nonce, inboxId) in zip(nonces, expected) {
				XCTAssertEqual(try generateInboxId(identity: identity, nonce: nonce), inboxId)
			}
			XCTAssertEqual(try generateInboxId(identity: identity), expected[0])
			XCTAssertEqual(try generateInboxId(identity: identity, nonce: nil), expected[0])
		}
	}

	func testInvalidInboxIdentityReturnsTypedErrorWithoutClient() throws {
		for identity in [
			PublicIdentity(identifier: "invalid", kind: .ethereum),
			PublicIdentity(identifier: "not hex", kind: .passkey),
		] {
			do {
				_ = try generateInboxId(identity: identity)
				XCTFail("Invalid identity returned an inbox ID")
			} catch XmtpError.InvalidArgument {}
		}
	}
}
