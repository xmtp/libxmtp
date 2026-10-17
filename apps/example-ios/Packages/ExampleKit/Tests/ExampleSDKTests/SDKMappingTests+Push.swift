import ExampleCore
@testable import ExampleSDK
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
