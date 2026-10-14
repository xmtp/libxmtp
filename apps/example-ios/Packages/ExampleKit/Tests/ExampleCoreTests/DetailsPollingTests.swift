import ExampleCore
import Foundation
import Testing

@MainActor private final class RemovedRoleQueries: GroupQueries {
	func load() async throws -> ConversationDetails {
		ConversationDetails(record: ConversationRecord(
			id: "group",
			kind: .group,
		))
	}

	func resolve(_ input: String) async throws -> GroupMember {
		GroupMember(id: input)
	}

	func mutate(_: DetailsMutation) async throws {}
	func permission(_: PermissionWrite) async throws {}
}

@MainActor private final class PollingAdmission { var valid = true }

@MainActor @Test func removedMemberEndsPendingRolePolling() async {
	let admission = PollingAdmission(); var job: Task<Void, Never>?
	let store = DetailsStore(
		queries: RemovedRoleQueries(),
		origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
		admit: { _ in admission.valid },
		run: { operation in job = Task { await operation() } },
	)
	store.send(.mutate(.admin("removed", true)))
	for _ in 0 ..< 10000 {
		if !store.state.busy {
			break
		}; await Task.yield()
	}
	#expect(store.state.pendingRoles.isEmpty)
	admission.valid = false; await job?.value
}
