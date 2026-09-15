import Foundation
import XCTest
@testable import XMTPiOS

final class NotificationConfigTests: XCTestCase {
	func testPreservesProviderTokensRulesAndMetadataAtTheBindingBoundary() {
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
				includeCommits: true,
				metadata: Data([0, 128, 255])
			).toFFI
			switch channel {
			case let .apns(token): XCTAssertEqual(config.channel, .apns(token: token))
			case let .fcm(token): XCTAssertEqual(config.channel, .fcm(token: token))
			case .http: XCTFail("Expected a provider channel")
			}
			XCTAssertEqual(config.consentStates, [.denied, .unknown])
			XCTAssertFalse(config.includeWelcomes)
			XCTAssertTrue(config.includeSyncGroups)
			XCTAssertTrue(config.includeCommits)
			XCTAssertEqual(config.metadata, Data([0, 128, 255]))

			let defaults = NotificationConfig(channel: channel).toFFI
			XCTAssertEqual(defaults.channel, config.channel)
			XCTAssertEqual(defaults.consentStates, [.allowed])
			XCTAssertTrue(defaults.includeWelcomes)
			XCTAssertFalse(defaults.includeSyncGroups)
			XCTAssertFalse(defaults.includeCommits)
			XCTAssertEqual(defaults.metadata, Data())
		}
	}
}
