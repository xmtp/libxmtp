import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
import XmtpSdk

struct SDKPushMappingTests {
	@Test func configurationPreservesEveryRule() {
		let value = pushConfiguration(token: "01abcdef")
		#expect(value.channel == .apns(token: "01abcdef"))
		#expect(value.consentStates == [.allowed, .unknown])
		#expect(value.includeWelcomes == true)
		#expect(value.includeSyncGroups == false)
		#expect(value.includeCommits == false)
	}

	@Test func terminalStatesStayDistinct() {
		#expect(pushState(.disabled) == .disabled)
		#expect(pushState(.enabled) == .enabled)
		#expect(pushState(.failed(error: .channelNotConfigured)) == .failed("Backend APNs channel is not configured"))
		let failures: [NotificationFailure] = [
			.permissionDenied,
			.invalidArgument,
			.outOfRange,
			.unimplemented,
			.channelNotConfigured,
		]
		#expect(Set(failures.map(pushFailure)).count == failures.count)
	}
}

/// The SDK owns the topic and sequence rules; these cases check the app's mapping of its result.
struct SDKPushEnvelopeTests {
	private func topic(_ kind: UInt8, _ byte: UInt8, count: Int = 32) -> String {
		Data([kind] + Array(repeating: byte, count: count)).base64EncodedString()
	}

	@Test func kindByteDecidesBetweenGroupAndWelcomeTopicsOfEqualLength() throws {
		let group = try #require(PushEnvelope(topic: topic(0, 0x33), sequence: "1"))
		#expect(group.kind == .group)
		#expect(group.identifier == String(repeating: "33", count: 32))
		#expect(group.topic == topic(0, 0x33))
		let welcome = try #require(PushEnvelope(topic: topic(1, 0x33), sequence: "1"))
		#expect(welcome.kind == .welcome)
		#expect(welcome.identifier == String(repeating: "33", count: 32))
		// A 16-byte group id has no topic, so a 17-byte 0x00 topic is refused.
		#expect(PushEnvelope(topic: topic(0, 0x11, count: 16), sequence: "1") == nil)
	}

	@Test func backendSequencesAndCanonicalTopics() {
		let group = topic(0, 255)
		#expect(PushEnvelope(topic: group, sequence: String(Int64.max))?.sequence == UInt64(Int64.max))
		#expect(PushEnvelope(topic: group, sequence: String(Int64.max))?.identifier == String(repeating: "ff", count: 32))
		for sequence in ["", "0", "-1", "+1", "1.0", " 1", "9223372036854775808", String(UInt64.max)] {
			#expect(PushEnvelope(topic: group, sequence: sequence) == nil)
		}
		#expect(PushEnvelope(topic: group + "\n", sequence: "1") == nil)
		for data in [
			Data(),
			Data([2] + Array(repeating: UInt8(0), count: 16)),
			Data([1] + Array(repeating: UInt8(0), count: 16)),
			Data([0] + Array(repeating: UInt8(0), count: 31)),
			Data([0] + Array(repeating: UInt8(0), count: 33)),
		] {
			#expect(PushEnvelope(topic: data.base64EncodedString(), sequence: "1") == nil)
		}
	}

	/// Push subscriptions carry only group and welcome topics (PUSH-215).
	@Test func otherTopicKindsAreIgnored() {
		for kind: UInt8 in [2, 3, 4] {
			#expect(PushEnvelope(topic: topic(kind, 1), sequence: "1") == nil)
		}
	}
}
