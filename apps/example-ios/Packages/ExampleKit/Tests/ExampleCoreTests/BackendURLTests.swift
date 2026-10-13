@testable import ExampleCore
import Foundation
import Testing

struct BackendURLTests {
	@Test(arguments: ["http://localhost:5050", "http://127.0.0.1", "http://[::1]:5050", "https://localhost"])
	func loopback(_ url: String) throws {
		#expect(try BackendURL(url).allowPrivateNetwork)
	}

	@Test(arguments: [
		"http://example.com",
		"https://example.com:0",
		"https://example.com:65536",
		"https://user@example.com",
		"https://example.com?q=x",
		"https://example.com#x",
		"ftp://localhost",
		"",
	])
	func rejects(_ url: String) {
		#expect(throws: (any Error).self) { try BackendURL(url) }
	}

	@Test func remoteAndNormalization() throws {
		let url = try BackendURL(" https://example.com/// "); #expect(url.value == "https://example.com"); #expect(!url
			.allowPrivateNetwork)
	}
}
