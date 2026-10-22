@testable import ExampleCore
import Foundation
import Testing

@MainActor struct PushPolicyTests {
	@Test(arguments: [false, true])
	func replacedControllerKeepsNewSameTagPublication(cancelOld: Bool) async throws {
		let fixture = PushFixture(); fixture.publisher.held = true
		let envelope = try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(), sequence: "1",
		))
		let old = Task { await fixture.controller.receive(envelope, lease: PushLease()) }
		for _ in 0 ..< 1000 where fixture.publisher.completion == nil {
			await Task.yield()
		}
		let oldID = try #require(fixture.publisher.heldSubmissionID)
		if cancelOld {
			old.cancel(); #expect(await old.value == false)
		}
		await fixture.controller.close()
		let fresh = PushController(configured: true, owner: fixture.owner, gateway: fixture.gateway,
		                           preferences: fixture.preferences, publisher: fixture.publisher, current: { fixture.owner },
		                           permitted: { true }, signedIn: { true })
		fixture.publisher.held = false
		#expect(await fresh.receive(envelope, lease: PushLease()))
		let freshID = try #require(fixture.publisher.lastSubmissionID)
		#expect(oldID != freshID)
		#expect(fixture.preferences.hasPosted(envelope.tag))
		fixture.publisher.complete()
		#expect(await old.value == false)
		#expect(fixture.publisher.visibleIDs == [freshID])
		#expect(await fresh.receive(envelope, lease: PushLease()) == false)
		#expect(fixture.publisher.posts == 2)
		await fresh.close()
	}

	@Test func registrationMatrix() {
		for configured in [false, true] {
			for signedIn in [false, true] {
				for permission in [false, true] {
					for appEnabled in [
						false,
						true,
					] {
						for token in [nil, "", "ab"] as [String?] {
							let input = PushRegistrationInput(
								configured: configured,
								signedIn: signedIn,
								permission: permission,
								appEnabled: appEnabled,
								token: token,
							)
							let expected: PushRegistrationAction = !configured ? .none :
								(signedIn && permission && appEnabled && token == "ab" ? .enable : .disable)
							#expect(input.action(registeredToken: "old", sdkEnabled: true) == expected)
						}
					}
				}
			}
		}
		let stable = PushRegistrationInput(configured: true, signedIn: true, permission: true, appEnabled: true, token: "ab")
		#expect(stable.action(registeredToken: "ab", sdkEnabled: true) == .none)
		#expect(stable.action(registeredToken: "ab", sdkEnabled: false) == .enable)
	}

	@Test func kindByteDecidesBetweenGroupAndWelcomeTopicsOfEqualLength() throws {
		let keyed = Data([0] + Array(repeating: UInt8(0x33), count: 32)).base64EncodedString()
		let group = try #require(PushEnvelope(topic: keyed, sequence: "1"))
		#expect(group.kind == .group)
		#expect(group.identifier == String(repeating: "33", count: 32))
		let legacy = Data([0] + Array(repeating: UInt8(0x11), count: 16)).base64EncodedString()
		#expect(PushEnvelope(topic: legacy, sequence: "1")?.kind == .group)
		let welcomeTopic = Data([1] + Array(repeating: UInt8(0x33), count: 32)).base64EncodedString()
		let welcome = try #require(PushEnvelope(topic: welcomeTopic, sequence: "1"))
		#expect(welcome.kind == .welcome)
		#expect(welcome.identifier == String(repeating: "33", count: 32))
	}

	@Test func canonicalTopicsAndTokenHex() {
		let group = Data([0] + Array(repeating: UInt8(255), count: 16)).base64EncodedString()
		#expect(PushEnvelope(topic: group, sequence: String(UInt64.max))?.identifier == String(repeating: "ff", count: 16))
		for sequence in ["", "-1", "+1", "1.0", " 1", "18446744073709551616"] {
			#expect(PushEnvelope(
				topic: group,
				sequence: sequence,
			) == nil)
		}
		for data in [
			Data(),
			Data([2] + Array(repeating: UInt8(0), count: 16)),
			Data([1] + Array(repeating: UInt8(0), count: 16)),
		] {
			#expect(PushEnvelope(topic: data.base64EncodedString(), sequence: "1") == nil)
		}
		#expect(PushEnvelope(topic: group + "\n", sequence: "1") == nil)
		for data in [
			Data([0] + Array(repeating: UInt8(0), count: 31)),
			Data([0] + Array(repeating: UInt8(0), count: 33)),
			Data([4] + Array(repeating: UInt8(0), count: 32)),
		] {
			#expect(PushEnvelope(topic: data.base64EncodedString(), sequence: "1") == nil)
		}
		#expect(PushEnvelope.hex(Data([0, 1, 10, 15, 16, 171, 255])) == "00010a0f10abff")
	}

	@Test func admissionAndDeduplication() async throws {
		let fixture = PushFixture()
		let envelope = try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(),
			sequence: "1",
		))
		fixture.preferences.enabled = false; #expect(await fixture.controller.receive(envelope, lease: PushLease()) == false)
		fixture.preferences.enabled = true; fixture.gateway.admitted = false; #expect(await fixture.controller.receive(
			envelope,
			lease: PushLease(),
		) == false)
		fixture.gateway.admitted = true; fixture.preferences.muted = true; #expect(await fixture.controller.receive(
			envelope,
			lease: PushLease(),
		) == false)
		fixture.preferences.muted = false; fixture
			.permission = false; #expect(await fixture.controller.receive(envelope, lease: PushLease()) == false)
		fixture.permission = true; #expect(await fixture.controller.receive(envelope, lease: PushLease()))
		#expect(await fixture.controller.receive(envelope, lease: PushLease()) == false)
		#expect(fixture.publisher.posts == 1)
	}

	@Test func welcomeMustNameCurrentInstallation() async throws {
		let fixture = PushFixture()
		let wrong = try #require(PushEnvelope(
			topic: Data([1] + Array(repeating: UInt8(2), count: 32)).base64EncodedString(),
			sequence: "1",
		))
		#expect(await fixture.controller.receive(wrong, lease: PushLease()) == false)
		let right = try #require(PushEnvelope(
			topic: Data([1] + Array(repeating: UInt8(1), count: 32)).base64EncodedString(),
			sequence: "1",
		))
		#expect(await fixture.controller.receive(right, lease: PushLease()))
	}

	@Test func suspendedReadsCannotOutliveOwnerPrivacyOrDeadline() async throws {
		for mutation in 0 ..< 4 {
			let fixture = PushFixture(); let lease = PushLease()
			let envelope = try #require(PushEnvelope(
				topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(),
				sequence: "1",
			))
			fixture.gateway.onRead = {
				if mutation == 0 {
					fixture.current = false
				}
				if mutation == 1 {
					fixture.controller.beginPrivacyChange()
				}
				if mutation == 2 {
					lease.cancel()
				}
				if mutation == 3 {
					fixture.preferences.muted = true
				}
			}
			#expect(await fixture.controller.receive(envelope, lease: lease) == false)
			#expect(fixture.publisher.posts == 0)
		}
	}

	@Test func latePlatformCompletionIsRemovedAfterPrivacyChange() async throws {
		let fixture = PushFixture(); fixture.publisher.held = true
		let envelope = try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(),
			sequence: "1",
		))
		let task = Task { await fixture.controller.receive(envelope, lease: PushLease()) }
		while fixture.publisher.completion == nil {
			await Task.yield()
		}
		fixture.controller.invalidatePrivacy(); fixture.publisher.complete()
		#expect(await task.value == false)
		#expect(fixture.publisher.removed.contains(envelope.tag))
		#expect(!fixture.preferences.hasPosted(envelope.tag))
	}

	@Test func cancellationDoesNotWaitForPlatformCallback() async throws {
		let fixture = PushFixture(); fixture.publisher.held = true; var finished = false
		let envelope = try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: UInt8(1), count: 16)).base64EncodedString(),
			sequence: "1",
		))
		let task = Task {
			let value = await fixture.controller.receive(envelope, lease: PushLease()); finished = true; return value
		}
		while fixture.publisher.completion == nil {
			await Task.yield()
		}
		task.cancel()
		for _ in 0 ..< 1000 where !finished {
			await Task.yield()
		}
		#expect(finished)
		fixture.publisher.complete()
		#expect(await task.value == false)
	}

	@Test func closeDisablesAfterLateEnable() async {
		let fixture = PushFixture(); fixture.gateway.holdEnable = true
		fixture.controller.tokenChanged(Data([1]))
		while fixture.gateway.enableContinuation == nil {
			await Task.yield()
		}
		fixture.current = false
		let closing = Task { await fixture.controller.close() }
		fixture.gateway.enableContinuation?.resume(returning: .enabled); fixture.gateway.enableContinuation = nil
		await closing.value
		#expect(fixture.gateway.calls == ["enable", "disable"])
	}

	@Test func mutedTapAndOldProfileAreRejected() async {
		let fixture = PushFixture()
		#expect(await fixture.controller.tap(PushRoute(profileID: UUID(), conversationID: "chat")) == nil)
		fixture.preferences.muted = true
		#expect(await fixture.controller
			.tap(PushRoute(profileID: fixture.owner.session.profileID, conversationID: "chat")) == nil)
	}
}
