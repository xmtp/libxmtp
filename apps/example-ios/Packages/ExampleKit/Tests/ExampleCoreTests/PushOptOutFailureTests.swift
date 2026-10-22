@testable import ExampleCore
import Foundation
import Testing

@MainActor struct PushOptOutFailureTests {
	@Test func failedOptOutBlocksNewAlertsAndTapRoutes() async throws {
		let fixture = PushFixture()
		fixture.preferences.failNotificationWrite = true
		fixture.controller.setEnabled(false)
		#expect(fixture.preferences.enabled)
		#expect(!fixture.controller.appEnabled)
		#expect(fixture.controller.status == "Cannot save notification setting. Try again.")
		#expect(try await fixture.controller.receive(envelope(), lease: PushLease()) == false)
		#expect(await fixture.controller
			.tap(PushRoute(profileID: fixture.owner.session.profileID, conversationID: "chat")) == nil)
		#expect(fixture.publisher.posts == 0)
		await fixture.controller.close()
	}

	@Test func refreshAndPreferenceReplacementCannotUndoFailedOptOut() async throws {
		let fixture = PushFixture()
		fixture.preferences.failNotificationWrite = true
		fixture.controller.setEnabled(false)
		let replacement = PushFakePreferences()
		fixture.controller.replacePreferences(replacement)
		#expect(!fixture.controller.appEnabled)
		fixture.controller.refresh(); fixture.controller.permissionChanged()
		#expect(!fixture.controller.appEnabled)
		#expect(try await fixture.controller.receive(envelope(), lease: PushLease()) == false)
		#expect(fixture.publisher.posts == 0)
		await fixture.controller.close()
		#expect(!fixture.gateway.calls.contains("enable"))
	}

	@Test func failedOptOutDisablesAfterHeldRegistrationCompletes() async throws {
		let fixture = PushFixture()
		fixture.gateway.holdEnable = true
		fixture.controller.tokenChanged(Data([1]))
		while fixture.gateway.enableContinuation == nil {
			await Task.yield()
		}
		fixture.preferences.failNotificationWrite = true
		fixture.controller.setEnabled(false)
		fixture.gateway.holdEnable = false
		fixture.gateway.enableContinuation?.resume(returning: .enabled)
		fixture.gateway.enableContinuation = nil
		for _ in 0 ..< 1000 where !fixture.gateway.calls.contains("disable") {
			await Task.yield()
		}
		#expect(fixture.gateway.calls == ["enable", "disable"])
		#expect(fixture.controller.status == "Cannot save notification setting. Try again.")
		#expect(try await fixture.controller.receive(envelope(), lease: PushLease()) == false)
		await fixture.controller.close()
	}

	@Test func onlySuccessfulExplicitOptInRestoresAdmission() async throws {
		let fixture = PushFixture()
		fixture.preferences.failNotificationWrite = true
		fixture.controller.setEnabled(false)
		fixture.controller.setEnabled(true)
		#expect(!fixture.controller.appEnabled)
		#expect(try await fixture.controller.receive(envelope(), lease: PushLease()) == false)
		fixture.preferences.failNotificationWrite = false
		fixture.controller.setEnabled(true)
		#expect(fixture.controller.appEnabled)
		#expect(try await fixture.controller.receive(envelope(), lease: PushLease()))
		#expect(fixture.publisher.posts == 1)
		await fixture.controller.close()
	}

	private func envelope() throws -> PushEnvelope {
		try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 32)).base64EncodedString(),
			sequence: "1",
		))
	}
}
