@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class DetailsPollGate {
	var waiters: [CheckedContinuation<Void, Never>] = []
	func wait() async throws {
		try Task.checkCancellation()
		await withCheckedContinuation { waiters.append($0) }
	}

	@discardableResult func releaseAll() -> Int {
		let pending = waiters; waiters = []
		pending.forEach { $0.resume() }
		return pending.count
	}
}

@MainActor private final class PendingDetailsQueries: GroupQueries {
	let loads = DetailsPollGate()
	var holdLoads = false
	var loadCount = 0
	var activeReads = 0
	var maximumActiveReads = 0
	var members: [GroupMember] = []
	func load() async throws -> ConversationDetails {
		loadCount += 1; activeReads += 1
		maximumActiveReads = max(maximumActiveReads, activeReads)
		defer { activeReads -= 1 }
		if holdLoads {
			try await loads.wait()
		}
		return ConversationDetails(record: ConversationRecord(id: "group", kind: .group), members: members)
	}

	func resolve(_ input: String) async throws -> GroupMember {
		GroupMember(id: input)
	}

	func mutate(_: DetailsMutation) async throws {}
	func permission(_: PermissionWrite) async throws {}
}

@MainActor private final class DetailsPollAdmission { var valid = true }

@MainActor private struct DetailsPollFixture {
	let scope: SessionScope
	let ticks: DetailsPollGate
	let queries: PendingDetailsQueries
	let admission: DetailsPollAdmission
	let store: DetailsStore
	init() {
		let scope = SessionScope(); let ticks = DetailsPollGate()
		let queries = PendingDetailsQueries(); let admission = DetailsPollAdmission()
		self.scope = scope; self.ticks = ticks; self.queries = queries; self.admission = admission
		store = DetailsStore(queries: queries,
		                     origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		                     admit: { _ in admission.valid }, run: { scope.add($0) }, pause: { try await ticks.wait() })
	}
}

@MainActor struct DetailsSinglePollerTests {
	private func eventually(_ condition: () -> Bool) async {
		for _ in 0 ..< 10000 {
			if condition() {
				return
			}
			await Task.yield()
		}
		#expect(condition(), "Gated details operation did not reach the expected phase")
	}

	@Test func multiplePendingActionsShareOnePollingReadAndAwaitItsDrain() async {
		let fixture = DetailsPollFixture()
		fixture.store.send(.mutate(.add("first")))
		await eventually { fixture.ticks.waiters.count == 1 }
		for name in ["second", "third"] {
			let previous = fixture.ticks.waiters.count
			fixture.store.send(.mutate(.add(name)))
			await eventually {
				!fixture.store.state.busy && (fixture.ticks.waiters.count > previous || fixture.scope.trackedTaskCount == 1)
			}
		}
		#expect(fixture.ticks.waiters.count == 1)
		#expect(fixture.store.state.pendingMembers.count == 3)
		#expect(fixture.queries.loadCount == 3)
		fixture.queries.holdLoads = true
		let released = fixture.ticks.releaseAll()
		await eventually { fixture.queries.loads.waiters.count == released }
		#expect(fixture.queries.activeReads == 1)
		#expect(fixture.queries.maximumActiveReads == 1)
		fixture.admission.valid = false
		var drained = false
		let closing = Task { await fixture.scope.cancelAndWait(); drained = true }
		await eventually { fixture.scope.trackedTaskCount == 0 }
		#expect(!drained, "Session teardown must await the in-flight SDK reload")
		fixture.queries.loads.releaseAll()
		await closing.value
		#expect(drained)
		#expect(fixture.queries.activeReads == 0)
		#expect(fixture.store.state.pendingMembers.count == 3)
	}

	@Test func settledPollerReleasesOwnershipForTheNextPendingAction() async {
		let fixture = DetailsPollFixture()
		fixture.store.send(.mutate(.add("first")))
		await eventually { fixture.ticks.waiters.count == 1 }
		fixture.queries.members = [GroupMember(id: "first")]
		fixture.ticks.releaseAll()
		await eventually { fixture.scope.trackedTaskCount == 0 }
		#expect(fixture.store.state.pendingMembers.isEmpty)
		fixture.store.send(.mutate(.add("second")))
		await eventually { fixture.ticks.waiters.count == 1 }
		#expect(fixture.store.state.pendingMembers.keys.contains("second"))
		fixture.admission.valid = false
		fixture.ticks.releaseAll()
		await fixture.scope.cancelAndWait()
	}

	@Test func cancellationDuringPauseCannotStartAnotherSDKRead() async {
		let fixture = DetailsPollFixture()
		fixture.store.send(.mutate(.add("first")))
		await eventually { fixture.ticks.waiters.count == 1 }
		let closing = Task { await fixture.scope.cancelAndWait() }
		await eventually { fixture.scope.trackedTaskCount == 0 }
		fixture.ticks.releaseAll()
		await closing.value
		#expect(fixture.queries.loadCount == 1)
		#expect(fixture.ticks.waiters.isEmpty)
	}

	@Test func staleRouteDuringPauseCannotStartAnotherSDKRead() async {
		let fixture = DetailsPollFixture()
		fixture.store.send(.mutate(.add("first")))
		await eventually { fixture.ticks.waiters.count == 1 }
		fixture.admission.valid = false
		fixture.ticks.releaseAll()
		await eventually { fixture.scope.trackedTaskCount == 0 }
		#expect(fixture.queries.loadCount == 1)
		#expect(fixture.ticks.waiters.isEmpty)
		await fixture.scope.cancelAndWait()
	}
}
