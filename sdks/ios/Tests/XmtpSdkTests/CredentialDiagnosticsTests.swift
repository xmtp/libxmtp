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

	func testNotificationChannelDiagnosticsHideValuesAndKeepStructuredFields() {
		let signingKey = Data([17, 61, 33])
		let cases: [(NotificationChannel, String)] = [
			(.apns(token: "notification-apns-sentinel-value"), "notification-apns-sentinel-value"),
			(.fcm(token: "notification-fcm-sentinel-value"), "notification-fcm-sentinel-value"),
			(
				.http(url: "https://notifications.invalid/notification-http-sentinel-value", signingKey: signingKey),
				"notification-http-sentinel-value"
			),
		]
		for (channel, secret) in cases {
			let config = NotificationConfig(channel: channel)
			let forms = [
				String(describing: channel),
				String(reflecting: channel),
				"\(channel)",
				String(describing: [channel]),
				String(reflecting: ["channel": channel]),
				String(describing: config),
				String(reflecting: config),
			]
			for value in forms {
				XCTAssertFalse(value.contains(secret), "Notification diagnostic exposed its value")
			}
			XCTAssertEqual(config.channel, channel)
			switch channel {
			case let .apns(token), let .fcm(token):
				XCTAssertEqual(token, secret)
			case let .http(url, key):
				XCTAssertTrue(url.contains(secret))
				XCTAssertEqual(key, signingKey)
			}
		}
	}
}
