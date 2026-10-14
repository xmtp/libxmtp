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

	@Test(arguments: [
		["HTTPS://EXAMPLE.COM:443", "https://example.com"],
		["HTTP://LOCALHOST:80/", "http://localhost"],
		["https://EXAMPLE.COM:443/API/v1///", "https://example.com/API/v1"],
		["http://[::1]:80", "http://[::1]"],
	])
	func equivalentEndpointsShareProfileKey(_ forms: [String]) throws {
		let first = try BackendURL(forms[0])
		let second = try BackendURL(forms[1])
		#expect(first == second)
		#expect(first.value == second.value)
	}

	@Test(arguments: [
		["https://example.com:444", "https://example.com"],
		["http://localhost:5050", "http://localhost"],
		["https://example.com/API", "https://example.com/api"],
	])
	func distinctEndpointsKeepSeparateProfileKeys(_ forms: [String]) throws {
		#expect(try BackendURL(forms[0]).value != BackendURL(forms[1]).value)
	}

	@Test func remoteAndNormalization() throws {
		let url = try BackendURL(" https://example.com/// "); #expect(url.value == "https://example.com"); #expect(!url
			.allowPrivateNetwork)
	}
}
