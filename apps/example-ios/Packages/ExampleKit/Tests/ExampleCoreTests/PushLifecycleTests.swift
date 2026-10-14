@testable import ExampleCore
import Foundation
import Testing

@MainActor struct PushLifecycleTests {
	@Test func foregroundDuringPushRestorePromotesOpenedSession() async {
		let fixture = PushSessionFixture()
		fixture.factory.hold = true
		let pending = Task { await fixture.session.restoreForPush() }
		while fixture.factory.waiter == nil {
			await Task.yield()
		}
		#expect(fixture.session.backgroundOnly)
		await fixture.session.restore()
		fixture.factory.waiter?.resume()
		#expect(await pending.value != nil)
		#expect(!fixture.session.backgroundOnly)
	}

	@Test func failedPushOpenReleasesReservationForRetry() async {
		let fixture = PushSessionFixture()
		fixture.secrets.values["wallet"] = nil
		#expect(await fixture.session.restoreForPush() == nil)
		#expect(fixture.session.state == .restoreFailed)
		#expect(!fixture.session.backgroundOnly)
		fixture.secrets.values["wallet"] = Data(repeating: 1, count: 32)
		#expect(await fixture.session.restoreForPush() != nil)
		#expect(fixture.factory.opens == 1)
	}

	@Test func stalePushFailureDoesNotReplaceNewerSession() async throws {
		let fixture = PushSessionFixture()
		fixture.factory.hold = true
		let pending = Task { await fixture.session.restoreForPush() }
		while fixture.factory.waiter == nil {
			await Task.yield()
		}
		let replacement = try #require(fixture.session.fence.replace(UUID()))
		fixture.factory.failure = .stale
		fixture.factory.waiter?.resume()
		#expect(await pending.value == nil)
		#expect(fixture.session.fence.accepts(replacement))
		#expect(fixture.session.state != .restoreFailed)
	}

	@Test func registrationFailureClearsOnRetryAndToken() {
		let fixture = PushFixture()
		fixture.controller.registrationFailed()
		#expect(fixture.controller.status == "APNs registration failed. Try again.")
		fixture.controller.registrationStarted()
		#expect(fixture.controller.status == "Waiting for APNs token")
		fixture.controller.registrationFailed()
		fixture.controller.tokenChanged(Data([1]))
		#expect(fixture.controller.status != "APNs registration failed. Try again.")
	}
}
