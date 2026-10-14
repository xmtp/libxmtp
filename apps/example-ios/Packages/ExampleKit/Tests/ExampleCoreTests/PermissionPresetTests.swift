@testable import ExampleCore
import Foundation
import Testing

@MainActor private final class PermissionFake: GroupQueries {
	var writes: [PermissionWrite] = []
	var loads = 0
	var failureAt: Int?
	var rejectMutation = false
	var loadFailure = false
	var pendingLeave = false
	var identityError: IdentityError?
	var ownRole: GroupRole = .superAdmin
	var ownPresent = true
	var mutationCount = 0
	var currentPolicies: [PermissionWrite]?
	func load() async throws -> ConversationDetails {
		loads += 1
		if loadFailure {
			throw DetailsError.failed
		}
		return ConversationDetails(
			record: .init(id: "group", kind: .group),
			members: (ownPresent ? [GroupMember(id: "own", role: ownRole)] : []) + [.init(id: "other", role: .superAdmin)],
			policies: currentPolicies ?? writes,
			preset: "Custom",
			pendingLeave: pendingLeave,
		)
	}

	func resolve(_ input: String) async throws -> GroupMember {
		if let identityError {
			throw identityError
		}
		return .init(id: input)
	}

	func mutate(_: DetailsMutation) async throws {
		mutationCount += 1
		if rejectMutation {
			throw DetailsError.failed
		}
	}

	func permission(_ write: PermissionWrite) async throws {
		if writes.count + 1 == failureAt {
			throw DetailsError.failed
		}
		writes.append(write)
	}
}

struct PermissionPresetTests {
	@Test @MainActor func nineWritesInOrder() async {
		let fake = PermissionFake()
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		fake.loads = 0
		store.send(.preset(true))
		await finish(store)
		#expect(fake.writes == [
			.init(.addMember, .admin),
			.init(.removeMember, .admin),
			.init(.addAdmin, .superAdmin),
			.init(.removeAdmin, .superAdmin),
			.init(.updateMetadata, .admin, .name),
			.init(.updateMetadata, .admin, .description),
			.init(.updateMetadata, .admin, .imageUrl),
			.init(.updateMetadata, .admin, .appData),
			.init(.updateMetadata, .admin, .disappearing),
		])
		#expect(fake.loads == 1)
		#expect(PermissionPreset.writes(adminOnly: false).first?.policy == .allow)
		#expect(PermissionPreset.writes(adminOnly: false).last?.policy == .admin)
	}

	@Test @MainActor func failureAtFiveReadsBackCommittedPolicies() async {
		let fake = PermissionFake()
		fake.failureAt = 5
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		fake.loads = 0
		store.send(.preset(false))
		await finish(store)
		#expect(fake.writes.count == 4)
		#expect(fake.loads == 1)
		#expect(store.state.details?.policies == fake.writes)
		#expect(store.state.error != nil)
	}

	@Test func disappearingRejectsOverflow() throws {
		#expect(try DisappearingRules.nanoseconds("0") == nil)
		#expect(try DisappearingRules.nanoseconds("3600") == 3_600_000_000_000)
		#expect(throws: DetailsError.invalidDuration) { try DisappearingRules.nanoseconds("9223372037") }
	}

	@Test @MainActor func rejectedAddLeavesNoPendingMember() async {
		let fake = PermissionFake()
		fake.rejectMutation = true
		let store = makeStore(fake)
		store.send(.mutate(.add("resolved-inbox")))
		await finish(store)
		#expect(store.state.pendingMembers.isEmpty)
		#expect(store.state.error != nil)
		#expect(fake.loads == 1)
	}

	@Test @MainActor func recipientErrorsKeepSpecificCopy() async {
		for error in [IdentityError.invalidAddress, .handleNotFound] {
			let fake = PermissionFake()
			fake.identityError = error
			let store = makeStore(fake)
			store.send(.mutate(.add("input")))
			await finish(store)
			#expect(store.state.error == error.rawValue)
		}
	}

	@Test func dmAvatarOmitsLeadingAtSign() {
		let details = ConversationDetails(record: .init(id: "dm", kind: .dm, peerInboxID: "1234", peerHandle: "maya#1234"))
		#expect(details.avatarLabel() == "M")
		#expect(details.avatarLabel(alias: "My friend") == "M")
		let group = ConversationDetails(record: .init(id: "group", kind: .group, name: "Protocol design"))
		#expect(group.avatarLabel() == "PD")
		#expect(group.avatarLabel(alias: "Release crew") == "RC")
		let unknown = ConversationDetails(record: .init(id: "dm", kind: .dm, peerInboxID: "0f3e8b21c5aa"))
		#expect(unknown.avatarLabel() == "0f")
		#expect(unknown.avatarLabel(alias: "Friend") == "F")
	}

	@Test @MainActor func initialLoadFailureOffersRetry() async {
		let fake = PermissionFake()
		fake.loadFailure = true
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		#expect(store.state.details == nil)
		#expect(store.state.initialLoadError == "Cannot update this conversation. Try again.")
		fake.loadFailure = false
		store.send(.refresh)
		await finish(store)
		#expect(store.state.details != nil)
		#expect(store.state.initialLoadError == nil)
	}

	@Test @MainActor func refreshedLeaveStateClearsPending() async {
		let fake = PermissionFake()
		fake.pendingLeave = true
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		#expect(store.state.pendingLeave)
		fake.pendingLeave = false
		store.send(.refresh)
		await finish(store)
		#expect(!store.state.pendingLeave)
	}

	@Test(arguments: [GroupRole.member, .admin])
	@MainActor func unauthorizedPresetNeverStarts(role: GroupRole) async {
		let fake = PermissionFake()
		fake.ownRole = role
		fake.currentPolicies = PermissionPreset.writes(adminOnly: false)
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		let details = store.state.details
		#expect(!store.canChangePermissionPreset)
		store.send(.preset(true))
		#expect(!store.state.busy)
		await finish(store)
		#expect(fake.writes.isEmpty)
		#expect(fake.loads == 1)
		#expect(store.state.details == details)
		#expect(store.state.error == nil)
	}

	@Test @MainActor func presetNeedsKnownOwnMembership() async {
		for own in [String?.none, "own"] {
			let fake = PermissionFake()
			fake.ownPresent = false
			let store = makeStore(fake, own: own)
			store.send(.preset(false))
			#expect(!store.state.busy)
			store.send(.refresh)
			await finish(store)
			store.send(.preset(false))
			#expect(!store.state.busy)
			await finish(store)
			#expect(fake.writes.isEmpty)
		}
	}

	@Test @MainActor func presetGuardDoesNotRestrictOtherPolicyMutations() async {
		let fake = PermissionFake()
		fake.ownRole = .member
		let store = makeStore(fake)
		store.send(.refresh)
		await finish(store)
		store.send(.mutate(.edit("Allowed name", "Allowed description")))
		await finish(store)
		#expect(fake.mutationCount == 1)
		#expect(fake.writes.isEmpty)
		#expect(store.state.error == nil)
	}

	@MainActor private func makeStore(_ fake: PermissionFake, own: String? = "own") -> DetailsStore {
		DetailsStore(
			queries: fake,
			ownInboxID: own,
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1),
			admit: { _ in true },
			run: { job in Task { await job() } },
		)
	}

	@MainActor private func finish(_ store: DetailsStore) async {
		for _ in 0 ..< 1000 {
			if !store.state.busy {
				return
			}
			await Task.yield()
		}
		Issue.record("Store did not finish")
	}
}
