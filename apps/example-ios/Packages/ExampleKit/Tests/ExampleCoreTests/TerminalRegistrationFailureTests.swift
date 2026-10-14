@testable import ExampleCore
import Foundation
import Testing

@MainActor struct TerminalRegistrationFailureTests {
	@Test(arguments: [
		"Backend APNs channel is not configured",
		"permission denied",
		"invalid argument",
		"out of range",
		"unsupported",
		"failed precondition",
	])
	func passiveCallbacksPreserveTerminalFailure(reason: String) async {
		let fixture = PushFixture()
		fixture.gateway.registrationState = .failed(reason)
		fixture.controller.tokenChanged(Data([1]))
		fixture.controller.registrationStarted()
		fixture.controller.permissionChanged()
		fixture.controller.refresh()
		await drain(fixture, reads: 6)
		#expect(fixture.gateway.calls.isEmpty)
		#expect(fixture.controller.status == reason)
		await fixture.controller.close()
	}

	@Test func explicitOptInAuthorizesOneTerminalAttempt() async {
		let fixture = PushFixture()
		fixture.gateway.registrationState = .failed("Terminal")
		fixture.gateway.enableResult = .failed("Terminal")
		fixture.controller.tokenChanged(Data([1]))
		await drain(fixture, reads: 2)
		#expect(fixture.gateway.calls.isEmpty)
		fixture.controller.setEnabled(true)
		await drain(fixture, reads: 3)
		#expect(fixture.gateway.calls == ["enable"])
		fixture.controller.refresh(); fixture.controller.refresh()
		await drain(fixture, reads: 5)
		#expect(fixture.gateway.calls == ["enable"])
		fixture.controller.setEnabled(true)
		await drain(fixture, reads: 6)
		#expect(fixture.gateway.calls == ["enable", "enable"])
		await fixture.controller.close()
	}

	@Test func explicitOptInWaitsForTokenWithoutGrantingFutureRetries() async {
		let fixture = PushFixture()
		fixture.gateway.registrationState = .failed("Terminal")
		fixture.gateway.enableResult = .failed("Terminal")
		fixture.controller.setEnabled(true)
		await drain(fixture, reads: 1)
		#expect(fixture.gateway.calls.isEmpty)
		fixture.controller.tokenChanged(Data([1]))
		await drain(fixture, reads: 3)
		#expect(fixture.gateway.calls == ["enable"])
		fixture.controller.tokenChanged(Data([2]))
		await drain(fixture, reads: 5)
		#expect(fixture.gateway.calls == ["enable"])
		await fixture.controller.close()
	}

	@Test func optInWhileAlreadyEnabledDoesNotAuthorizeLaterTerminalRetry() async {
		let fixture = PushFixture()
		fixture.controller.tokenChanged(Data([1]))
		await drain(fixture, reads: 2)
		#expect(fixture.gateway.calls == ["enable"])
		fixture.gateway.registrationState = .enabled
		fixture.controller.setEnabled(true)
		await drain(fixture, reads: 3)
		#expect(fixture.gateway.calls == ["enable"])
		fixture.gateway.registrationState = .failed("Terminal")
		fixture.controller.refresh()
		await drain(fixture, reads: 4)
		#expect(fixture.gateway.calls == ["enable"])
		await fixture.controller.close()
	}

	@Test func failedPreferenceWriteDoesNotAuthorizeTerminalRetry() async {
		let fixture = PushFixture()
		fixture.gateway.registrationState = .failed("Terminal")
		fixture.preferences.failNotificationWrite = true
		fixture.controller.tokenChanged(Data([1]))
		await drain(fixture, reads: 2)
		fixture.controller.setEnabled(true)
		await drain(fixture, reads: 3)
		#expect(fixture.gateway.calls.isEmpty)
		#expect(fixture.controller.status == "Cannot save notification setting. Try again.")
		await fixture.controller.close()
	}

	private func drain(_ fixture: PushFixture, reads: Int) async {
		for _ in 0 ..< 1000 where fixture.gateway.stateReads < reads {
			await Task.yield()
		}
		#expect(fixture.gateway.stateReads >= reads)
	}
}
