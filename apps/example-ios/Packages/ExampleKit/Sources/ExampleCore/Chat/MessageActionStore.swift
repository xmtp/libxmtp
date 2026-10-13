import Foundation
import Observation

public enum MessageAction: Sendable {
	case load(String), react(ChatMessage, String), requestDelete(ChatMessage),
	     confirmDelete,
	     cancelDelete, retry(ChatMessage)
}

public struct MessageActionState: Equatable,
	Sendable
{
	public var message: ChatMessage?
	public var deleteTarget: ChatMessage?
	public var error: String?
	public var busy =
		false
}

@MainActor @Observable public final class MessageActionStore {
	public private(set) var state = MessageActionState()
	private let queries: any ChatQueries
	private let coordinator: SendCoordinator
	private let origin: DestinationOrigin
	private let admit: (
		DestinationOrigin
	)
		-> Bool
	private let own: String
	private let changed: () async -> Void
	private let jobs = InFlightTasks()
	public var allowed = false
	public init(
		queries: any ChatQueries,
		coordinator: SendCoordinator,
		origin: DestinationOrigin,
		admit: @escaping (DestinationOrigin) -> Bool,
		own: String,
		changed: @escaping () async -> Void,
	) {
		self.queries = queries
		self.coordinator = coordinator
		self.origin = origin
		self.admit = admit
		self.own = own
		self
			.changed = changed
	}

	public func send(_ action: MessageAction) {
		guard admit(origin) else { return }
		switch action {
		case .cancelDelete: state.deleteTarget = nil
		case .load: state.busy = true
			jobs.start { await self.run(action) }
		case let .requestDelete(row): if allowed, row.sender == own, row.delivery == .published,
		                                 !row.content.isDeleted
			{
				state.deleteTarget = row
			}
		default: guard allowed else { return }
			state.busy = true
			let deleteTarget = state.deleteTarget
			jobs.start { await self.run(action, deleteTarget: deleteTarget) }
		}
	}

	public func wait() async {
		await jobs.wait()
	}

	public func close() async {
		await jobs.close()
	}

	private func apply(_ body: (inout MessageActionState) -> Void) {
		guard admit(origin) else { return }
		body(&state)
	}

	private func run(_ action: MessageAction, deleteTarget: ChatMessage? = nil) async {
		do {
			switch action {
			case let .load(id):
				let row = try await queries
					.message(id: id)
				if let row {
					let conversation = try await queries.conversation(id: row.conversationID)
					guard admit(origin)
					else { return }
					allowed = conversation.consent == .allowed
				}
				apply { $0.message = row }
			case let .react(row, emoji):
				let mine = MessageMapper.reactions(row.reactions, own: own).contains { $0.emoji == emoji && $0.mine }
				_ = try await coordinator.queue(
					draft: SendDraft(conversationID: row.conversationID),
					content: .reaction(parent: row.id, emoji: emoji, remove: mine),
					admission: { self.admit(self.origin) && self.allowed },
				)
				let refreshed = try await queries.message(id: row.id)
				apply { $0.message = refreshed }
			case .confirmDelete: if let row = deleteTarget {
					try await queries.delete(messageID: row.id)
				}
				apply { $0.deleteTarget = nil }
			case let .retry(row): guard row.sender == own else { throw ChatError.rejected }
				try await coordinator.retry(
					conversationID: row.conversationID,
					messageID: row.id,
					admission: { self.admit(self.origin) && self.allowed },
				)
			default: break
			}
			apply { $0.busy = false
				$0.error = nil
			}
			await changed()
		} catch { apply { $0.busy = false
			$0.error = "Cannot complete this message action. Try again."
		} }
	}
}
