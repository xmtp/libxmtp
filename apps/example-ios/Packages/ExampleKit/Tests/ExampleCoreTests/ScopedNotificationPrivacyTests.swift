@testable import ExampleCore
import Foundation
import Testing

@MainActor struct ScopedNotificationPrivacyTests {
	@Test(arguments: [ScopedPushFailure.lookup, .write])
	func failedConversationChangePreservesUnrelatedAlertsAndAdmission(failure: ScopedPushFailure) async throws {
		let fixture = ScopedPushFixture()
		fixture.gateway.failure = failure
		fixture.gateway.holdLookup = true
		fixture.publisher.seed(tag: "old-a", conversationID: fixture.firstID)
		fixture.publisher.seed(tag: "old-b", conversationID: fixture.secondID)
		fixture.publisher.seed(tag: "old-welcome", conversationID: nil)
		let oldOther = try fixture.groupEnvelope(2, sequence: "1")
		fixture.publisher.heldTags.insert(oldOther.tag)
		let heldOther = Task { await fixture.controller.receive(oldOther, lease: PushLease()) }
		await waitUntil { fixture.publisher.callbacks[oldOther.tag] != nil }
		let change = Task { await fixture.controller.setConversationEnabled(false, id: fixture.firstID) }
		await waitUntil { fixture.gateway.lookupWaiter != nil }
		#expect(fixture.publisher.visible.contains("old-b"))
		#expect(fixture.publisher.visible.contains("old-welcome"))
		#expect(fixture.publisher.allRemovals == 0)
		#expect(try await fixture.controller.receive(fixture.groupEnvelope(2, sequence: "2"), lease: PushLease()))
		#expect(try await fixture.controller.receive(fixture.groupEnvelope(1, sequence: "2"), lease: PushLease()) == false)
		#expect(try await fixture.controller.receive(fixture.welcomeEnvelope(), lease: PushLease()))
		#expect(await fixture.controller.tap(PushRoute(
			profileID: fixture.owner.session.profileID,
			conversationID: fixture.secondID,
		)) != nil)
		fixture.gateway.lookupWaiter?.resume(); fixture.gateway.lookupWaiter = nil
		#expect(await change.value == false)
		fixture.publisher.complete(oldOther.tag)
		#expect(await heldOther.value)
		#expect(fixture.publisher.visible.contains("old-b"))
		#expect(fixture.publisher.visible.contains(oldOther.tag))
		#expect(!fixture.publisher.removed.contains(oldOther.tag))
		await fixture.controller.close()
	}

	@Test func scopedChangeRejectsOnlyAffectedLateCompletion() async throws {
		let fixture = ScopedPushFixture()
		let first = try fixture.groupEnvelope(1, sequence: "1"), second = try fixture.groupEnvelope(2, sequence: "1")
		fixture.publisher.heldTags = [first.tag, second.tag]
		let firstPost = Task { await fixture.controller.receive(first, lease: PushLease()) }
		let secondPost = Task { await fixture.controller.receive(second, lease: PushLease()) }
		await waitUntil { fixture.publisher.callbacks.count == 2 }
		#expect(await fixture.controller.setConversationEnabled(false, id: fixture.firstID))
		fixture.publisher.complete(first.tag); fixture.publisher.complete(second.tag)
		#expect(await firstPost.value == false)
		#expect(await secondPost.value)
		#expect(fixture.publisher.removed.contains(first.tag))
		#expect(!fixture.publisher.removed.contains(second.tag))
		#expect(fixture.publisher.visible.contains(second.tag))
		await fixture.controller.close()
	}

	@Test func globalPrivacyStillRejectsEveryLateCompletion() async throws {
		let fixture = ScopedPushFixture()
		let first = try fixture.groupEnvelope(1, sequence: "1"), second = try fixture.groupEnvelope(2, sequence: "1")
		fixture.publisher.heldTags = [first.tag, second.tag]
		let firstPost = Task { await fixture.controller.receive(first, lease: PushLease()) }
		let secondPost = Task { await fixture.controller.receive(second, lease: PushLease()) }
		await waitUntil { fixture.publisher.callbacks.count == 2 }
		fixture.controller.beginPrivacyChange()
		#expect(fixture.publisher.allRemovals == 1)
		fixture.publisher.complete(first.tag); fixture.publisher.complete(second.tag)
		#expect(await firstPost.value == false); #expect(await secondPost.value == false)
		#expect(fixture.publisher.visible.isEmpty)
		fixture.controller.endPrivacyChange()
		await fixture.controller.close()
	}

	private func waitUntil(_ ready: () -> Bool) async {
		for _ in 0 ..< 1000 where !ready() {
			await Task.yield()
		}
		#expect(ready())
	}
}

enum ScopedPushFailure: Error { case lookup, write }
@MainActor private final class ScopedPushFixture {
	let firstID = String(repeating: "01", count: 16)
	let secondID = String(repeating: "02", count: 16)
	let owner = PushOwner(
		session: SessionKey(profileID: UUID(), generation: 1),
		installation: String(repeating: "01", count: 32),
	)
	let gateway = ScopedPushGateway()
	let preferences = ScopedPushPreferences()
	let publisher = ScopedPushPublisher()
	lazy var controller = PushController(configured: true, owner: owner, gateway: gateway, preferences: preferences,
	                                     publisher: publisher, current: { [unowned self] in owner }, permitted: { true },
	                                     signedIn: { true })
	func groupEnvelope(_ byte: UInt8, sequence: String) throws -> PushEnvelope {
		try #require(PushEnvelope(
			topic: Data([0] + Array(repeating: byte, count: 16)).base64EncodedString(),
			sequence: sequence,
		))
	}

	func welcomeEnvelope() throws -> PushEnvelope {
		try #require(PushEnvelope(
			topic: Data([1] + Array(repeating: UInt8(1), count: 32)).base64EncodedString(),
			sequence: "1",
		))
	}
}

@MainActor private final class ScopedPushGateway: NotificationGateway {
	let installation = String(repeating: "01", count: 32)
	var failure: ScopedPushFailure?
	var holdLookup = false
	var lookupWaiter: CheckedContinuation<Void, Never>?
	var disabledConversations: Set<String> = []
	func state() -> PushRegistrationState {
		.disabled
	}

	func enable(token _: String) async -> PushRegistrationState {
		.enabled
	}

	func disable() async {}
	func conversation(id: String) async throws -> PushConversation? {
		if holdLookup, id == String(repeating: "01", count: 16) {
			holdLookup = false
			await withCheckedContinuation { lookupWaiter = $0 }
			if failure == .lookup {
				throw ScopedPushFailure.lookup
			}
		}
		return PushConversation(id: id, logicalKey: id, admitted: !disabledConversations.contains(id))
	}

	func setEnabled(_ enabled: Bool, conversationID: String) async throws {
		if failure == .write {
			throw ScopedPushFailure.write
		}
		if enabled {
			disabledConversations.remove(conversationID)
		} else {
			disabledConversations.insert(conversationID)
		}
	}
}

@MainActor private final class ScopedPushPreferences: PushPreferences {
	var notificationsEnabled = true
	var mutes: Set<String> = []
	var posted: Set<String> = []
	func setNotificationsEnabled(_ enabled: Bool) {
		notificationsEnabled = enabled
	}

	func muted(for logicalKey: String) -> Bool {
		mutes.contains(logicalKey)
	}

	func setMute(_ muted: Bool, for logicalKey: String) {
		if muted {
			mutes.insert(logicalKey)
		} else {
			mutes.remove(logicalKey)
		}
	}

	func hasPosted(_ tag: String) -> Bool {
		posted.contains(tag)
	}

	func recordPosted(_ tag: String) {
		posted.insert(tag)
	}
}

@MainActor private final class ScopedPushPublisher: PushPublisher {
	var routes: [String: String?] = [:]
	var submissions: [UUID: String] = [:]
	var visible: Set<String> = []
	var removed: Set<String> = []
	var heldTags: Set<String> = []
	var callbacks: [String: @MainActor (Bool) -> Void] = [:]
	var allRemovals = 0
	func seed(tag: String, conversationID: String?) {
		routes[tag] = .some(conversationID); visible.insert(tag)
	}

	func post(submissionID: UUID, tag: String, route: PushRoute, completion: @escaping @MainActor (Bool) -> Void) {
		submissions[submissionID] = tag
		routes[tag] = .some(route.conversationID)
		if heldTags.contains(tag) {
			callbacks[tag] = completion
		} else {
			visible.insert(tag); completion(true)
		}
	}

	func complete(_ tag: String) {
		visible.insert(tag); let callback = callbacks.removeValue(forKey: tag); callback?(true)
	}

	func remove(submissionID: UUID) {
		guard let tag = submissions[submissionID] else { return }
		remove(tag: tag)
	}

	private func remove(tag: String) {
		removed.insert(tag); visible.remove(tag)
	}

	func remove(conversationID: String) {
		for (tag, id) in routes where id == conversationID {
			remove(tag: tag)
		}
	}

	func removeAll() {
		allRemovals += 1; visible.removeAll()
	}
}
