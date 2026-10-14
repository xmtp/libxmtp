import Foundation
import Observation

public struct DetailsState: Equatable, Sendable {
	public var details: ConversationDetails?
	public var recipient: GroupMember?
	public var busy = false
	public var error: String?
	public var initialLoadError: String? {
		details == nil && !busy ? error : nil
	}

	public var pendingMembers: [String: GroupMember] = [:]
	public var pendingRoles: [String: GroupRole] = [:]
	public var pendingRemoval: Set<String> = []
	public var pendingLeave = false
	public var blocked = false
	public init() {}
}

@MainActor @Observable public final class DetailsStore {
	public private(set) var state = DetailsState()
	private var lookupAttempt = 0
	private let queries: any GroupQueries
	private let origin: DestinationOrigin
	private let admit: @MainActor (DestinationOrigin) -> Bool
	private let run: @MainActor (@escaping @MainActor @Sendable () async -> Void) -> Void
	public init(
		queries: any GroupQueries,
		origin: DestinationOrigin,
		admit: @escaping @MainActor (DestinationOrigin) -> Bool,
		run: @escaping @MainActor (@escaping @MainActor @Sendable () async -> Void) -> Void,
	) {
		self.queries = queries
		self.origin = origin
		self.admit = admit
		self.run = run
	}

	public enum Action: Sendable {
		case refresh, mutate(DetailsMutation), preset(Bool)
	}

	public func resolvePreview(_ input: String) {
		lookupAttempt += 1
		let attempt = lookupAttempt
		state.recipient = nil
		state.error = nil
		guard !input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
		run { [weak self] in
			guard let self else { return }
			do { try await Task.sleep(for: .milliseconds(300))
				guard attempt == lookupAttempt, admit(origin) else { return }
				let recipient = try await queries.resolve(input)
				guard attempt == lookupAttempt else { return }
				apply { $0.recipient = recipient }
			} catch is CancellationError {} catch { guard attempt == lookupAttempt else { return }
				apply { $0.error = (error as? IdentityError)?.rawValue ?? "No XMTP account matches this recipient." }
			}
		}
	}

	public func send(_ action: Action) {
		guard !state.busy, admit(origin) else { return }
		state.busy = true
		state.error = nil
		run { [weak self] in await self?.perform(action) }
	}

	private func apply(_ change: (inout DetailsState) -> Void) {
		guard admit(origin) else { return }
		change(&state)
	}

	private func reload() async throws {
		let details = try await queries.load()
		apply { state in
			state.details = details
			state.pendingMembers = state.pendingMembers.filter { id, _ in !details.members.contains { $0.id == id } }
			state.pendingRoles = state.pendingRoles
				.filter { id, role in
					guard let member = details.members.first(where: { $0.id == id }) else { return false }
					return member.role != role
				}
			state.pendingRemoval = state.pendingRemoval.filter { id in details.members.contains { $0.id == id } }
			state.pendingLeave = details.pendingLeave
		}
	}

	private func perform(_ action: Action) async {
		do {
			switch action {
			case .refresh: break
			case let .preset(adminOnly):
				for write in PermissionPreset.writes(adminOnly: adminOnly) {
					guard admit(origin) else { throw CancellationError() }
					try await queries.permission(write)
				}
			case let .mutate(action): try await performMutation(action)
			}
			try await reload()
		} catch is CancellationError { } catch {
			// A preset is several commits. Always show the committed policies after a partial failure.
			do { try await reload() } catch {}
			apply { $0.error = (error as? IdentityError)?.rawValue ?? "Cannot update this conversation. Try again." }
		}
		apply { $0.busy = false }
		await pollPending()
	}

	private func performMutation(_ action: DetailsMutation) async throws {
		var resolved = action
		var addedMember: GroupMember?
		if case let .add(input) = action {
			let member = try await queries.resolve(input)
			guard admit(origin) else { throw CancellationError() }
			resolved = .add(member.id)
			addedMember = member
		}
		guard admit(origin) else { throw CancellationError() }
		try await queries.mutate(resolved)
		recordPending(resolved, addedMember: addedMember)
	}

	private func recordPending(_ action: DetailsMutation, addedMember: GroupMember?) {
		apply { state in
			if let addedMember {
				state.pendingMembers[addedMember.id] = addedMember
			}
			switch action {
			case let .remove(id): state.pendingRemoval.insert(id)
			case let .admin(id, enabled): state.pendingRoles[id] = enabled ? .admin : .member
			case .leave: state.pendingLeave = true
			case .block: state.blocked = true
			default: break
			}
		}
	}

	private func pollPending() async {
		while admit(origin),
		      !state.pendingMembers.isEmpty || !state.pendingRoles.isEmpty || !state.pendingRemoval.isEmpty || state
		      .pendingLeave
		{
			do { try await Task.sleep(for: .seconds(1))
				guard admit(origin) else { return }
				try await reload()
			} catch { return }
		}
	}
}
