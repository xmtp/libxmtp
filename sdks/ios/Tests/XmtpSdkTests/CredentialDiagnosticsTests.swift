import XCTest
import XmtpSdk

final class CredentialDiagnosticsTests: XCTestCase {
	func testCredentialDiagnosticsHideValueAndKeepStructuredFields() {
		let secret = "credential-display-sentinel-value"
		var credential = Credential(name: "api-key", value: secret, expiresAtSeconds: 123)
		let forms = [
			String(describing: credential),
			String(reflecting: credential),
			"\(credential)",
			String(describing: [credential]),
			String(reflecting: ["credential": credential]),
		]
		for value in forms {
			XCTAssertFalse(value.contains(secret), "Credential diagnostic exposed its value")
		}
		XCTAssertEqual(credential.value, secret)
		XCTAssertEqual(credential.name, "api-key")
		XCTAssertEqual(credential.expiresAtSeconds, 123)
		credential.value = "replacement-display-sentinel-value"
		XCTAssertFalse(
			String(reflecting: credential).contains(credential.value),
			"Mutated credential diagnostic exposed its value",
		)
		XCTAssertEqual(credential.value, "replacement-display-sentinel-value")
	}
}
