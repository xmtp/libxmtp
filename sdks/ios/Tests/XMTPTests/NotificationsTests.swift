import Foundation
import XCTest
import XMTPiOS
import XMTPTestHelpers

@available(iOS 15, *)
final class NotificationsTests: XCTestCase {
	private func httpConfig() throws -> NotificationConfig {
		try NotificationConfig(channel: .http(
			url: "https://example.com/xmtp-notification-test",
			signingKey: Crypto.secureRandomBytes(count: 32)
		))
	}

	func testRegistersHttpAndResetsGroupAndDmOverrides() async throws {
		let fixtures = try await fixtures()
		defer { try? fixtures.cleanUpDatabases() }
		let client = try XCTUnwrap(fixtures.alixClient)
		let group = try await client.conversations.newGroup(with: [fixtures.boClient.inboxID])
		let dm = try await client.conversations.findOrCreateDm(with: fixtures.boClient.inboxID)
		var state = try await client.notificationState()
		XCTAssertEqual(state, .disabled)
		state = try await client.enableNotifications(httpConfig())
		XCTAssertEqual(state, .enabled)
		state = try await client.notificationState()
		XCTAssertEqual(state, .enabled)

		var enabled = try await group.notificationsEnabled()
		XCTAssertTrue(enabled)
		try await group.setNotifications(.disabled)
		enabled = try await group.notificationsEnabled()
		XCTAssertFalse(enabled)
		try await group.setNotifications(.default)
		enabled = try await group.notificationsEnabled()
		XCTAssertTrue(enabled)
		enabled = try await dm.notificationsEnabled()
		XCTAssertTrue(enabled)
		try await dm.setNotifications(.disabled)
		enabled = try await dm.notificationsEnabled()
		XCTAssertFalse(enabled)
		try await dm.setNotifications(.default)
		enabled = try await dm.notificationsEnabled()
		XCTAssertTrue(enabled)

		var config = try httpConfig()
		config.consentStates = []
		config.includeWelcomes = false
		config.includeSyncGroups = false
		config.includeCommits = true
		config.metadata = Data([0, 128, 255])
		_ = try await client.enableNotifications(config)
		for conversation in [Conversation.group(group), .dm(dm)] {
			enabled = try await conversation.notificationsEnabled()
			XCTAssertFalse(enabled)
			try await conversation.setNotifications(.enabled)
			enabled = try await conversation.notificationsEnabled()
			XCTAssertTrue(enabled)
			try await conversation.setNotifications(.default)
			enabled = try await conversation.notificationsEnabled()
			XCTAssertFalse(enabled)
		}
		try await client.disableNotifications()
		state = try await client.notificationState()
		XCTAssertEqual(state, .disabled)
	}

	func testReturnsTypedFailuresForUnconfiguredChannels() async throws {
		let fixtures = try await fixtures()
		defer { try? fixtures.cleanUpDatabases() }
		let client = try XCTUnwrap(fixtures.alixClient)
		let channels: [NotificationChannel] = [.apns(token: String(repeating: "a", count: 64)), .fcm(token: "token")]
		for channel in channels {
			do {
				_ = try await client.enableNotifications(NotificationConfig(channel: channel))
				XCTFail("An unconfigured channel must fail")
			} catch let error as NotificationError {
				XCTAssertEqual(error.code, "NotificationError::ChannelNotConfigured")
			}
			let state = try await client.notificationState()
			guard case let .failed(error) = state else {
				return XCTFail("Expected failed state")
			}
			XCTAssertEqual(error.code, "NotificationError::ChannelNotConfigured")
			try await client.disableNotifications()
			let disabled = try await client.notificationState()
			XCTAssertEqual(disabled, .disabled)
		}
	}
}
