import Foundation
import XCTest
import XmtpSdk

/// Notification registration through the Swift package, against the backend.
final class NotificationRegistrationTests: XCTestCase {
	func testEnableAndDisableNotifications() async throws {
		let client = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		XCTAssertEqual(try client.notificationState(), .disabled)
		do {
			_ = try await client.enableNotifications(config: NotificationConfig(
				channel: .http(url: "https://example.com/xmtp-notification-test", signingKey: Data([1])),
			))
			XCTFail("A one-byte signing key was accepted")
		} catch XmtpError.InvalidArgument {}
		XCTAssertEqual(try client.notificationState(), .disabled)

		let enabled = try await client.enableNotifications(config: NotificationConfig(
			channel: .http(
				url: "https://example.com/xmtp-notification-test",
				signingKey: Data((0 ..< 32).map { _ in UInt8.random(in: 0 ... 255) }),
			),
		))
		XCTAssertEqual(enabled, .enabled)
		XCTAssertEqual(try client.notificationState(), .enabled)
		try await client.disableNotifications()
		XCTAssertEqual(try client.notificationState(), .disabled)
		try await client.end()
	}
}
