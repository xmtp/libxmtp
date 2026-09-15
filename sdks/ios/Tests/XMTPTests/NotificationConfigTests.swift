import Foundation
import XCTest
@testable import XMTPiOS

final class NotificationConfigTests: XCTestCase {
	func testPreservesProviderTokensAndRulesAtTheBindingBoundary() {
		let channels: [NotificationChannel] = [
			.apns(token: String(repeating: "ab", count: 32)),
			.fcm(token: "fcm:token-_123"),
		]
		for channel in channels {
			let config = NotificationConfig(
				channel: channel,
				consentStates: [.denied, .unknown],
				includeWelcomes: false,
				includeSyncGroups: true,
				includeCommits: true
			).toFFI
			switch channel {
			case let .apns(token): XCTAssertEqual(config.channel, .apns(token: token))
			case let .fcm(token): XCTAssertEqual(config.channel, .fcm(token: token))
			case .http: XCTFail("Expected a provider channel")
			}
			XCTAssertEqual(config.consentStates, [.denied, .unknown])
			XCTAssertEqual(config.includeWelcomes, false)
			XCTAssertEqual(config.includeSyncGroups, true)
			XCTAssertEqual(config.includeCommits, true)

			let defaults = NotificationConfig(channel: channel).toFFI
			XCTAssertEqual(defaults.channel, config.channel)
			XCTAssertEqual(defaults.consentStates, [.allowed])
			XCTAssertEqual(defaults.includeWelcomes, true)
			XCTAssertEqual(defaults.includeSyncGroups, false)
			XCTAssertEqual(defaults.includeCommits, false)
		}
	}
}
