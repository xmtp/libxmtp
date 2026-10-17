import Foundation
import Observation

public struct SharedMediaState: Equatable, Sendable {
	public var rows: [ChatMessage] = []
	public var allowed = false
	public var loading = false
	public var error: String?
}

public enum SharedMediaAction: Sendable {
	case load
}

@MainActor @Observable public final class SharedMediaStore {
	public private(set) var state = SharedMediaState()
	private let id: String
	private let queries: any ChatQueries
	private let origin: DestinationOrigin
	private let admit: (DestinationOrigin) -> Bool
	private var revision: UInt64 = 0
	private var task: Task<Void, Never>?
	private let jobs = InFlightTasks()
	public init(
		id: String,
		queries: any ChatQueries,
		origin: DestinationOrigin,
		admit: @escaping (DestinationOrigin) -> Bool,
	) {
		self.id = id
		self.queries = queries
		self.origin = origin
		self.admit = admit
	}

	public func send(_: SharedMediaAction) {
		guard admit(origin) else { return }
		task?.cancel(); revision += 1
		let expected = revision
		state.allowed = false; state.error = nil; state.loading = true
		task = jobs.start { [self] in
			do {
				let conversation = try await queries.conversation(id: id)
				try Task.checkCancellation()
				let rows = try await queries
					.media(conversationID: id)
				try Task.checkCancellation()
				guard expected == revision else { return }
				apply {
					$0.allowed = conversation.consent == .allowed
					$0.rows = rows
					$0.loading = false
				}
			} catch is CancellationError {} catch {
				guard expected == revision, !Task.isCancelled else { return }
				apply { $0.error = "Cannot load media and files. Try again."
					$0.loading = false
				}
			}
		}
	}

	public func close() async {
		await jobs.close()
	}

	private func apply(_ change: (inout SharedMediaState) -> Void) {
		guard admit(origin) else { return }
		change(&state)
	}
}
